//! 远程探针业务层：result fan-out broadcast、confirm token TTL 缓存、
//! per-admin 限速。
//!
//! 这一层把"客户端 POST /api/probe/results 收到一条结果" 转给 "admin
//! 的 GET /:batch_id/stream SSE 长连接" — admin handler 订阅 broadcast，
//! 客户端 handler publish。

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// admin SSE 流接收到的单条结果消息（与 client POST 的 body 字段一一对应）。
/// 字段名走 camelCase，与前端 SSE 解析保持一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResultPayload {
    pub device_id: String,
    pub request_id: String,
    pub batch_id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_json: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    #[serde(default)]
    pub truncated: bool,
}

/// D 类受控写二次确认的 ticket。
/// 客户端首次回 `confirm_required` 后写入；admin 调 confirm 端点时校验
/// token + device_id 后 5 位匹配，通过则推送 ProbeConfirm SSE 给客户端。
#[derive(Debug, Clone)]
pub struct ConfirmTicket {
    pub token: String,
    pub device_id: String,
    pub expires_at: std::time::Instant,
}

/// confirm token 默认 TTL，与设计文档 §5.4 一致。
pub const CONFIRM_TTL_SECS: u64 = 60;

#[derive(Clone, Default)]
pub struct ProbeService {
    /// batch_id → broadcast sender（admin SSE 端订阅）。
    /// 容量 256 足够吸收一次性 broadcast 多设备结果的瞬时峰值。
    result_tx: Arc<DashMap<String, broadcast::Sender<ProbeResultPayload>>>,
    /// request_id → confirm ticket（TTL 60s，超时由 sweeper 清理）。
    pending_confirm: Arc<DashMap<String, ConfirmTicket>>,
    /// admin_id → 最近 60s 内调用时间戳。in-memory，重启清零（设计接受）。
    admin_calls: Arc<DashMap<String, Vec<std::time::Instant>>>,
}

/// per-admin 限速错误。
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RateLimitError {
    #[error("admin rate limit exceeded")]
    Exceeded,
}

const BROADCAST_CAPACITY: usize = 256;

impl ProbeService {
    pub fn new() -> Self {
        Self::default()
    }

    /// admin SSE handler 调用：订阅指定 batch 的结果流。
    /// 即使当前没有 publisher 也会就地创建 sender —— admin 先订阅、client
    /// 后投递的并发顺序是常态，必须支持。
    pub fn subscribe_batch(&self, batch_id: &str) -> broadcast::Receiver<ProbeResultPayload> {
        let sender = self
            .result_tx
            .entry(batch_id.to_string())
            .or_insert_with(|| broadcast::channel(BROADCAST_CAPACITY).0);
        sender.subscribe()
    }

    /// 客户端 POST /api/probe/results 处理完毕后调用：把结果广播给该 batch
    /// 的所有 admin 订阅者。若当前 admin 还没建立 SSE 流，结果落库后会丢失
    /// "实时"展示窗口（admin 可通过 GET /:request_id 历史查询补回）；这是
    /// 设计上的可接受妥协（替代方案是 replay buffer，但内存占用不可控）。
    pub fn publish_result(&self, payload: ProbeResultPayload) {
        if let Some(sender) = self.result_tx.get(&payload.batch_id) {
            // broadcast::Sender::send 在无订阅者时返回 Err，忽略即可。
            let _ = sender.send(payload);
        }
    }

    /// admin SSE 流结束时调用：清理 sender。若仍有其他订阅者会被
    /// broadcast::Receiver::Closed 唤醒（属预期）。
    pub fn drop_batch(&self, batch_id: &str) {
        self.result_tx.remove(batch_id);
    }

    /// 客户端首次回传 `status=confirm_required` 时，admin probe results 路由
    /// 写入 ticket；TTL 默认 60s。
    pub fn issue_confirm(&self, request_id: &str, ticket: ConfirmTicket) {
        self.pending_confirm.insert(request_id.to_string(), ticket);
    }

    /// admin POST /confirm 校验：device 后 5 位匹配 + 未过期。
    /// confirm_token 是客户端 ↔ 服务端的内部凭证，admin 看不到也不需要 —— 仅
    /// 防止客户端误把别的 SSE 事件当作 confirm 处理。
    ///
    /// 通过则**消费**（remove）并返回 ticket；不通过返回 ConfirmError。
    ///
    /// 原子 check-and-take：先 `remove` 取得 owned ticket（DashMap 在分片锁内
    /// 原子摘除，并发场景下只有一个调用能拿到 `Some`，其余拿到 `None` →
    /// `NotFound`），再做过期/后缀校验。这保证任一 `request_id` 至多被成功
    /// 消费一次，杜绝 get→remove 之间的 TOCTOU 双重消费。
    ///
    /// 后缀不匹配时把 ticket 原样 `insert` 回去，维持"失败不消费"语义，允许
    /// admin 在 TTL 内重试；过期/后缀不匹配同 sweeper 语义保持一致。
    pub fn consume_confirm(
        &self,
        request_id: &str,
        provided_device_suffix: &str,
    ) -> Result<ConfirmTicket, ConfirmError> {
        // 原子摘除：拿到 owned ticket 的调用独占消费权。
        let (_rid, ticket) = self
            .pending_confirm
            .remove(request_id)
            .ok_or(ConfirmError::NotFound)?;

        // 过期 → 已摘除，直接丢弃（无需写回，与 sweeper 语义一致）。
        if ticket.expires_at <= std::time::Instant::now() {
            return Err(ConfirmError::Expired);
        }
        // 后 5 位匹配（device_id 长度不足 5 时取全部）。按字符计数取后缀，
        // 避免 device_id 以多字节字符结尾时字节切片越过 char boundary 而 panic。
        let expected: String = {
            let chars: Vec<char> = ticket.device_id.chars().collect();
            let start = chars.len().saturating_sub(5);
            chars[start..].iter().collect()
        };
        if !expected.eq_ignore_ascii_case(provided_device_suffix) {
            // 后缀不匹配不算成功消费 → 写回 ticket，允许 TTL 内重试。
            self.pending_confirm
                .insert(request_id.to_string(), ticket);
            return Err(ConfirmError::SuffixMismatch);
        }

        // 通过 → ticket 已摘除，直接返回（已被消费）。
        Ok(ticket)
    }

    /// 扫描过期 ticket。返回 (expired_request_ids)，admin probe TTL sweeper
    /// 据此把 DB 状态推进到 'expired'。
    pub fn sweep_expired_confirms(&self) -> Vec<String> {
        let now = std::time::Instant::now();
        let expired: Vec<String> = self
            .pending_confirm
            .iter()
            .filter_map(|kv| {
                if kv.value().expires_at <= now {
                    Some(kv.key().clone())
                } else {
                    None
                }
            })
            .collect();
        for rid in &expired {
            self.pending_confirm.remove(rid);
        }
        expired
    }

    #[cfg(test)]
    pub fn pending_confirm_count(&self) -> usize {
        self.pending_confirm.len()
    }

    /// per-admin 滑动窗口限速：检查最近 60s 内调用次数 <= max_per_min；通过则
    /// 记录本次时间戳。失败返回 Err(Exceeded)。
    ///
    /// 不持久化（重启清零）。每次调用顺带清理本 admin 的过期时间戳，避免内存
    /// 无界增长。
    pub fn check_and_record_admin_call(
        &self,
        admin_id: &str,
        max_per_min: u32,
    ) -> Result<(), RateLimitError> {
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(60);

        let mut entry = self.admin_calls.entry(admin_id.to_string()).or_default();
        // 清理过期时间戳
        entry.retain(|t| now.duration_since(*t) <= window);
        if entry.len() as u32 >= max_per_min {
            return Err(RateLimitError::Exceeded);
        }
        entry.push(now);
        Ok(())
    }

    #[cfg(test)]
    pub fn admin_call_count(&self, admin_id: &str) -> usize {
        self.admin_calls
            .get(admin_id)
            .map(|e| e.value().len())
            .unwrap_or(0)
    }
}

/// confirm 端点错误枚举。
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ConfirmError {
    #[error("confirm ticket 不存在或已被消费")]
    NotFound,
    #[error("confirm ticket 已过期")]
    Expired,
    #[error("device_id 后 5 位不匹配")]
    SuffixMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribe_then_publish_delivers_payload() {
        let svc = ProbeService::new();
        let mut rx = svc.subscribe_batch("b-1");
        svc.publish_result(ProbeResultPayload {
            device_id: "d-1".into(),
            request_id: "r-1".into(),
            batch_id: "b-1".into(),
            status: "ok".into(),
            result_json: Some(serde_json::json!({"ua": "test"})),
            stderr: None,
            duration_ms: Some(42),
            truncated: false,
        });
        let got = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.request_id, "r-1");
        assert_eq!(got.status, "ok");
    }

    #[tokio::test]
    async fn publish_without_subscriber_is_noop() {
        let svc = ProbeService::new();
        // 无 sender → 无 send 调用 → 无 panic。
        svc.publish_result(ProbeResultPayload {
            device_id: "d-1".into(),
            request_id: "r-1".into(),
            batch_id: "b-orphan".into(),
            status: "ok".into(),
            result_json: None,
            stderr: None,
            duration_ms: None,
            truncated: false,
        });
    }

    #[tokio::test]
    async fn drop_batch_removes_sender() {
        let svc = ProbeService::new();
        let _ = svc.subscribe_batch("b-temp");
        svc.drop_batch("b-temp");
        // 再次 subscribe 应该拿到新 sender（不会复用旧的）—— 验证 drop 生效
        let _rx2 = svc.subscribe_batch("b-temp");
    }

    fn ticket(device_id: &str, ttl_secs: u64) -> ConfirmTicket {
        ConfirmTicket {
            token: "secret-token-123".into(),
            device_id: device_id.into(),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(ttl_secs),
        }
    }

    #[test]
    fn issue_and_consume_confirm_happy_path() {
        let svc = ProbeService::new();
        svc.issue_confirm("r-1", ticket("abcde12345", 60));
        let consumed = svc.consume_confirm("r-1", "12345").unwrap();
        assert_eq!(consumed.device_id, "abcde12345");
        // 已消费 → 再次 consume 失败
        let again = svc.consume_confirm("r-1", "12345");
        assert!(matches!(again, Err(ConfirmError::NotFound)));
    }

    #[test]
    fn consume_confirm_wrong_suffix() {
        let svc = ProbeService::new();
        svc.issue_confirm("r-2", ticket("device-XYZAB", 60));
        let err = svc.consume_confirm("r-2", "wrong").unwrap_err();
        assert_eq!(err, ConfirmError::SuffixMismatch);
        // 不消费失败的 ticket
        assert_eq!(svc.pending_confirm_count(), 1);
    }

    #[test]
    fn consume_confirm_expired() {
        let svc = ProbeService::new();
        // ttl=0 → 立即过期
        svc.issue_confirm("r-4", ticket("dev-AAAAA", 0));
        std::thread::sleep(std::time::Duration::from_millis(20));
        let err = svc.consume_confirm("r-4", "AAAAA").unwrap_err();
        assert_eq!(err, ConfirmError::Expired);
        // 过期已被清理
        assert_eq!(svc.pending_confirm_count(), 0);
    }

    #[test]
    fn sweep_expired_returns_and_removes() {
        let svc = ProbeService::new();
        svc.issue_confirm("r-old", ticket("d-1", 0));
        svc.issue_confirm("r-new", ticket("d-2", 60));
        std::thread::sleep(std::time::Duration::from_millis(20));
        let expired = svc.sweep_expired_confirms();
        assert_eq!(expired, vec!["r-old".to_string()]);
        assert_eq!(svc.pending_confirm_count(), 1);
    }

    #[test]
    fn rate_limit_allows_under_threshold_and_blocks_at_threshold() {
        let svc = ProbeService::new();
        let max = 3_u32;
        // 前 3 次通过
        for _ in 0..3 {
            assert!(svc.check_and_record_admin_call("admin-A", max).is_ok());
        }
        // 第 4 次被拒
        let err = svc.check_and_record_admin_call("admin-A", max).unwrap_err();
        assert_eq!(err, RateLimitError::Exceeded);
        // 不影响其他 admin
        assert!(svc.check_and_record_admin_call("admin-B", max).is_ok());
        assert_eq!(svc.admin_call_count("admin-A"), 3);
        assert_eq!(svc.admin_call_count("admin-B"), 1);
    }

    #[test]
    fn case_insensitive_suffix_match() {
        let svc = ProbeService::new();
        svc.issue_confirm("r-5", ticket("dev-AbCdE", 60));
        // 输入大小写不敏感
        assert!(svc.consume_confirm("r-5", "abcde").is_ok());
    }
}
