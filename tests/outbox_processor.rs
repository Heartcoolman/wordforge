//! S2-1：outbox 异步消费 worker 集成测试。
//!
//! 覆盖 worker 编排：失败事件退避重排（不丢、不立即死信）、空 outbox 安静 noop。
//! 「重启不丢」「死信移转」等存储层机制在 `src/store/operations/outbox.rs` 单测覆盖。

use std::sync::Arc;

use learning_backend::amas::config::AMASConfig;
use learning_backend::amas::engine::AMASEngine;
use learning_backend::config::Config;
use learning_backend::state::AppState;
use learning_backend::store::Store;
use learning_backend::workers::outbox_processor;
use tokio::sync::broadcast;

fn ensure_secrets() {
    let s = "test_secret_that_is_at_least_32_characters_long_ok";
    for k in ["JWT_SECRET", "ADMIN_JWT_SECRET", "REFRESH_JWT_SECRET"] {
        if std::env::var(k).map(|v| v.len() < 32).unwrap_or(true) {
            std::env::set_var(k, s);
        }
    }
}

fn build_state(store: Arc<Store>) -> AppState {
    ensure_secrets();
    let cfg = Config::from_env();
    let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
    let (tx, _) = broadcast::channel(8);
    AppState::new(store, amas, &cfg, tx, false)
}

#[tokio::test]
async fn worker_reschedules_unknown_event_then_keeps_pending() {
    let tmp = tempfile::tempdir().unwrap();
    let store =
        Arc::new(Store::open(tmp.path().join("outbox_wf.db").to_str().unwrap(), 5000, 2).unwrap());
    store.run_migrations().unwrap();
    // 未知类型事件 → process_one 必然 Err → reschedule（attempts=1，仍 pending，未死信）。
    store.enqueue_outbox_event("bogus_type", "{}").unwrap();
    let state = build_state(store.clone());

    outbox_processor::run(&state).await;

    let stats = store.outbox_stats().unwrap();
    assert_eq!(stats.pending, 1, "未知事件应被退避重排而非删除");
    assert_eq!(stats.dead_letter, 0, "首次失败不应进死信");
    // 退避后 next_retry_at 在未来，当前时刻不可再领取
    let now = chrono::Utc::now().to_rfc3339();
    assert!(store.claim_due_outbox_events(&now, 10).unwrap().is_empty());
    // 用远未来时间领取，验证 attempts 已 bump
    let future = (chrono::Utc::now() + chrono::Duration::seconds(7200)).to_rfc3339();
    let due = store.claim_due_outbox_events(&future, 10).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].attempts, 1);
}

#[tokio::test]
async fn worker_drains_empty_outbox_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(
        Store::open(
            tmp.path().join("outbox_empty.db").to_str().unwrap(),
            5000,
            2,
        )
        .unwrap(),
    );
    store.run_migrations().unwrap();
    let state = build_state(store.clone());
    // 空 outbox：run 应安静返回、无 panic。
    outbox_processor::run(&state).await;
    assert_eq!(store.outbox_stats().unwrap().pending, 0);
}
