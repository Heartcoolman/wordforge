use crate::store::{Store, StoreError};

/// 展示历史保留窗口（7 天）。按 shown_at 时间窗清理，不区分会话状态。
/// 7 天足以覆盖跨设备 / 周末间断学习的恢复 UX；陈旧 active session 也会被清掉，
/// 用户即使重连也会得到新一轮 SRS 评估，符合记忆模型预期。
const SHOWN_WORDS_RETENTION_SECS: i64 = 7 * 24 * 60 * 60;

pub async fn run(store: &Store) {
    tracing::debug!("session_cleanup: start");
    let store = store.clone();
    match crate::blocking::run_blocking("worker.session_cleanup", move || {
        let cleaned = store.cleanup_expired_sessions()?;
        let pruned_shown =
            store.prune_session_shown_words_older_than(SHOWN_WORDS_RETENTION_SECS)?;
        Ok::<_, StoreError>((cleaned, pruned_shown))
    })
    .await
    {
        Ok(Ok((cleaned, pruned_shown))) => tracing::info!(
            cleaned,
            pruned_shown,
            "session_cleanup: done"
        ),
        Ok(Err(e)) => tracing::error!(error=%e, "session_cleanup failed"),
        Err(e) => tracing::error!(error=%e, "session_cleanup task failed"),
    }
}
