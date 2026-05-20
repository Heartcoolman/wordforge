//! probe_executions 表的 retention cron。
//!
//! 每 24 小时跑一次，删除 dispatched_at < (now - retention_days) 的行。
//! retention_days 来自 ProbeConfig（默认 60）。enabled=false 时跳过执行
//! （不读不写，避免无谓 IO）。

use std::time::Duration;

use tokio::sync::broadcast;

use crate::state::AppState;

const TICK_INTERVAL_SECS: u64 = 24 * 60 * 60;

pub async fn run(state: AppState, mut shutdown_rx: broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
    // 第一次 tick 立即触发（tokio::interval 默认行为）；启动时即跑一次 cleanup，
    // 之后每 24h 跑一次。
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = sweep_once(&state).await {
                    tracing::warn!(error = %e, "probe_cleanup tick failed");
                }
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

async fn sweep_once(state: &AppState) -> Result<(), String> {
    let cfg = state.config().probe.clone();
    if !cfg.enabled {
        return Ok(());
    }
    let cutoff = chrono::Utc::now()
        - chrono::Duration::days(cfg.retention_days as i64);
    let cutoff_str = cutoff.to_rfc3339();
    let store = state.store().clone();
    let cutoff_clone = cutoff_str.clone();
    let deleted = tokio::task::spawn_blocking(move || store.delete_probe_older_than(&cutoff_clone))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
        .map_err(|e| format!("delete_probe_older_than: {e}"))?;
    if deleted > 0 {
        tracing::info!(
            deleted,
            cutoff = %cutoff_str,
            retention_days = cfg.retention_days,
            "probe_cleanup swept old probe_executions"
        );
    }
    Ok(())
}
