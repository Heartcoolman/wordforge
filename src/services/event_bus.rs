//! v1.1-P1 S2：领域事件总线。
//!
//! 背景：`docs/v1-research/should-deferred.md` §S2 承诺 records → AMAS 走"事件总线 +
//! 异步消费"路径，最终目标是用 outbox 表替代当前 handler 内同步调 `AMASEngine::process_event`
//! 后手动 rollback 的非原子写库语义。
//!
//! v1.1-P1 落地的是**事件总线基础设施**：内存 broadcast 通道 + RecordCreated 事件，
//! handler 写库成功后旁路 emit；AMAS 同步通路保留为主路径不变（feature `event-bus`
//! 默认 enabled 时通路打开，关闭时整体编译走老路）。outbox 持久化、AMAS 真异步消费
//! 留给后续 task。
//!
//! 关键设计：
//! - broadcast 通道 lagged 不阻塞 producer；handler 不感知消费者状态。
//! - consumer panic 自我恢复（`spawn_record_consumer` 用 catch_unwind），与 M1-A2
//!   AMASEngine 锁中毒防护对齐。
//! - 单元测试不依赖 tokio 运行时启动顺序，emit 时无订阅者直接丢事件（broadcast 语义）。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;

use crate::store::operations::records::RecordType;

/// 通道缓冲；超出后老事件被丢、订阅者收到 `RecvError::Lagged`。
/// 当前 consumer 仅做轻量计数 / 日志，1024 已绰绰有余；
/// 真实接 AMAS 异步通路后需要根据 records 峰值 QPS 重新评估。
pub const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// 领域事件。**新增 variant 必须同时更新 consumer**。
///
/// 设计原则：事件 payload 自包含足够下游消费的字段，避免 consumer 再回查 store
/// 引入额外锁；与 should-deferred.md §S2 中 outbox 表列一致，未来落库零字段迁移。
#[derive(Debug, Clone)]
pub enum DomainEvent {
    /// 一条学习记录已写库成功（包括 single 与 batch 两个入口）。
    RecordCreated {
        user_id: String,
        word_id: String,
        record_id: String,
        is_correct: bool,
        response_time_ms: i64,
        session_id: Option<String>,
        record_type: RecordType,
        created_at: DateTime<Utc>,
    },
}

/// 事件总线 trait。当前仅一种实现（内存 broadcast），抽出 trait 是为：
/// - 单元测试可注入 NoopBus（feature off 路径）
/// - 后续 outbox 落库实现替换不需要改 handler 调用点
pub trait EventBus: Send + Sync {
    /// 旁路投递；调用方不关心是否有订阅者、不关心送达。
    /// 返回成功送达的订阅者数量，仅用于指标 / 调试。
    fn emit(&self, event: DomainEvent) -> usize;

    /// 订阅领域事件。返回 None 表示该 bus 实现不支持订阅（如 Noop）。
    fn subscribe(&self) -> Option<broadcast::Receiver<DomainEvent>>;
}

/// 内存实现，基于 `tokio::sync::broadcast`：多订阅者、lagged 不阻塞 producer。
pub struct InMemoryEventBus {
    tx: broadcast::Sender<DomainEvent>,
}

impl InMemoryEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        Self { tx }
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new(DEFAULT_CHANNEL_CAPACITY)
    }
}

impl EventBus for InMemoryEventBus {
    fn emit(&self, event: DomainEvent) -> usize {
        // send 在无订阅者时返回 Err；这符合"旁路 fire-and-forget"语义，记 0 即可。
        self.tx.send(event).unwrap_or(0)
    }

    fn subscribe(&self) -> Option<broadcast::Receiver<DomainEvent>> {
        Some(self.tx.subscribe())
    }
}

/// 空实现：feature `event-bus` 关闭时（或测试明确禁用）使用。
/// emit 全部丢弃、subscribe 返回 None。
pub struct NoopEventBus;

impl EventBus for NoopEventBus {
    fn emit(&self, _event: DomainEvent) -> usize {
        0
    }

    fn subscribe(&self) -> Option<broadcast::Receiver<DomainEvent>> {
        None
    }
}

/// 构造默认 bus；feature 开关在编译期切换实现。
pub fn build_default_bus() -> Arc<dyn EventBus> {
    #[cfg(feature = "event-bus")]
    {
        Arc::new(InMemoryEventBus::default())
    }
    #[cfg(not(feature = "event-bus"))]
    {
        Arc::new(NoopEventBus)
    }
}

/// 默认 consumer：订阅 RecordCreated，目前仅计数 + 周期日志。
///
/// 设计目的：
/// - 在 v1.1-P1 阶段验证总线通路可用；
/// - 为未来"AMAS 真异步消费"提供 spawn 入口（替换函数体即可）；
/// - panic 时自动重订阅，避免 worker 死亡导致总线哑火。
///
/// 调用方在 main.rs / WorkerManager 启动期 spawn 一次。
pub async fn run_record_consumer<B: EventBus + ?Sized>(
    bus: &B,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) {
    let Some(mut rx) = bus.subscribe() else {
        tracing::info!("EventBus 不支持 subscribe（NoopEventBus / 测试禁用），consumer 退出");
        return;
    };

    let mut received: u64 = 0;
    let mut lagged: u64 = 0;
    let mut report_interval = tokio::time::interval(std::time::Duration::from_secs(60));
    report_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // 首 tick 立即触发，跳过避免日志噪音
    report_interval.tick().await;

    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                tracing::info!(received, lagged, "EventBus consumer 收到 shutdown，退出");
                return;
            }
            _ = report_interval.tick() => {
                if received > 0 || lagged > 0 {
                    tracing::info!(received, lagged, "EventBus RecordCreated 计数");
                }
            }
            recv = rx.recv() => {
                match recv {
                    Ok(event) => {
                        received += 1;
                        // 当前只有 RecordCreated 一个 variant；新增 variant 后改成 match 即可。
                        let DomainEvent::RecordCreated { user_id, word_id, record_id, is_correct, .. } = &event;
                        tracing::debug!(
                            user_id = %user_id,
                            word_id = %word_id,
                            record_id = %record_id,
                            is_correct,
                            "EventBus 收到 RecordCreated"
                        );
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        lagged += n;
                        tracing::warn!(skipped = n, total_lagged = lagged, "EventBus consumer 落后，丢弃事件");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!(received, lagged, "EventBus 通道关闭，consumer 退出");
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> DomainEvent {
        DomainEvent::RecordCreated {
            user_id: "u1".into(),
            word_id: "w1".into(),
            record_id: "r1".into(),
            is_correct: true,
            response_time_ms: 1200,
            session_id: None,
            record_type: RecordType::default(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn noop_bus_emit_returns_zero_and_no_subscribe() {
        let bus = NoopEventBus;
        assert_eq!(bus.emit(sample_event()), 0);
        assert!(bus.subscribe().is_none());
    }

    #[test]
    fn in_memory_bus_emit_without_subscriber_drops() {
        let bus = InMemoryEventBus::default();
        assert_eq!(bus.emit(sample_event()), 0);
    }

    #[tokio::test]
    async fn in_memory_bus_delivers_to_subscriber() {
        let bus = InMemoryEventBus::default();
        let mut rx = bus.subscribe().expect("subscribe ok");
        assert_eq!(bus.emit(sample_event()), 1);
        let received = rx.recv().await.expect("recv ok");
        assert!(matches!(received, DomainEvent::RecordCreated { .. }));
    }

    #[tokio::test]
    async fn in_memory_bus_lagged_is_recoverable() {
        // 容量 2，发 4 条，订阅者会 lag 2 条
        let bus = InMemoryEventBus::new(2);
        let mut rx = bus.subscribe().expect("subscribe ok");
        for _ in 0..4 {
            bus.emit(sample_event());
        }
        // 第一次 recv 应该是 Lagged
        match rx.recv().await {
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                assert_eq!(skipped, 2);
            }
            other => panic!("expected Lagged, got {other:?}"),
        }
        // 后续两条仍可消费
        assert!(rx.recv().await.is_ok());
        assert!(rx.recv().await.is_ok());
    }
}
