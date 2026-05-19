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

/// D 类受控写二次确认的 ticket（M3 才会用，M1 占位）。
#[derive(Debug, Clone)]
pub struct ConfirmTicket {
    pub token: String,
    pub device_id: String,
    pub expires_at: std::time::Instant,
}

#[derive(Clone, Default)]
pub struct ProbeService {
    /// batch_id → broadcast sender（admin SSE 端订阅）。
    /// 容量 256 足够吸收一次性 broadcast 多设备结果的瞬时峰值。
    result_tx: Arc<DashMap<String, broadcast::Sender<ProbeResultPayload>>>,
    /// M3 用：request_id → confirm ticket（TTL 60s）。
    #[allow(dead_code)]
    pending_confirm: Arc<DashMap<String, ConfirmTicket>>,
    /// M4 用：admin_id → 最近 60s 内调用时间戳。
    #[allow(dead_code)]
    admin_calls: Arc<DashMap<String, Vec<std::time::Instant>>>,
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
}
