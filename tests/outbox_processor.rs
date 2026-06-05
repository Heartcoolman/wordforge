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
async fn worker_dead_letters_unknown_event_immediately() {
    let tmp = tempfile::tempdir().unwrap();
    let store =
        Arc::new(Store::open(tmp.path().join("outbox_wf.db").to_str().unwrap(), 5000, 2).unwrap());
    store.run_migrations().unwrap();
    // W1-2：未知类型事件 = 永久错误（毒丸）→ 跳过退避直接进死信，不空跑 5 次。
    store.enqueue_outbox_event("bogus_type", "{}").unwrap();
    let state = build_state(store.clone());

    outbox_processor::run(&state).await;

    let stats = store.outbox_stats().unwrap();
    assert_eq!(stats.pending, 0, "永久错误应立即移出 outbox");
    assert_eq!(stats.dead_letter, 1, "永久错误应立即进死信");
    // 未来时间也领不到（已不在 outbox，未空跑退避）。
    let future = (chrono::Utc::now() + chrono::Duration::seconds(7200)).to_rfc3339();
    assert!(store.claim_due_outbox_events(&future, 10).unwrap().is_empty());
    // 死信明细可列出，记录 attempts=1（首次失败即进死信，非耗尽 5 次）。
    let dl = store.list_dead_letter(10).unwrap();
    assert_eq!(dl.len(), 1);
    assert_eq!(dl[0].attempts, 1, "毒丸首次失败即进死信，attempts=1");
    assert_eq!(dl[0].event_type, "bogus_type");
}

#[tokio::test]
async fn worker_dead_letters_unparseable_payload_immediately() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Arc::new(
        Store::open(tmp.path().join("outbox_bad.db").to_str().unwrap(), 5000, 2).unwrap(),
    );
    store.run_migrations().unwrap();
    // record_created 但 payload 无法反序列化 → 永久错误 → 立即死信。
    store
        .enqueue_outbox_event("record_created", "{not valid json")
        .unwrap();
    let state = build_state(store.clone());

    outbox_processor::run(&state).await;

    let stats = store.outbox_stats().unwrap();
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.dead_letter, 1);
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
