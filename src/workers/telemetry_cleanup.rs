//! telemetry_events / telemetry_summaries 两表的 retention cron。
//!
//! 每 24 小时跑一次，删除 server_ts < (now - telemetry_retention_days) 的行。
//! retention_days 来自 Config（默认 90）。仅 leader 跑（清理非幂等，多实例会
//! 并发删同一批行；统一由 leader 单点执行）。best-effort，失败仅告警不 panic。

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
                    tracing::warn!(error = %e, "telemetry_cleanup tick failed");
                }
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

/// 计算 retention cutoff 字符串。**必须与入库侧 `datetime('now')` 同格式**
/// （`YYYY-MM-DD HH:MM:SS`，空格分隔），否则 TEXT 字典序 `server_ts < cutoff` 比较会因
/// 'T'(0x54) vs 空格(0x20) 失配 —— cutoff 当天行被整体误删（过删 ~1 天）。
fn retention_cutoff_str(now: chrono::DateTime<chrono::Utc>, retention_days: u64) -> String {
    let cutoff = now - chrono::Duration::days(retention_days as i64);
    cutoff.format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn sweep_once(state: &AppState) -> Result<(), String> {
    let cfg = state.config();
    // 清理非幂等，仅 leader 执行，避免多实例并发删同一批行。
    if !cfg.worker.is_leader {
        return Ok(());
    }
    let retention_days = cfg.telemetry_retention_days;
    let cutoff_str = retention_cutoff_str(chrono::Utc::now(), retention_days);
    let store = state.store().clone();
    let cutoff_clone = cutoff_str.clone();
    let deleted =
        tokio::task::spawn_blocking(move || store.delete_telemetry_older_than(&cutoff_clone))
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))?
            .map_err(|e| format!("delete_telemetry_older_than: {e}"))?;
    if deleted > 0 {
        tracing::info!(
            deleted,
            cutoff = %cutoff_str,
            retention_days,
            "telemetry_cleanup swept old telemetry rows"
        );
    }

    // m061：摄取拒绝留痕表同 retention 清理。该表 server_ts 为 RFC3339（'T' 分隔 + 时区后缀，
    // 见 insert_ingest_rejection），与上方空格格式 cutoff 不同，必须用 RFC3339 cutoff 比较。
    let cutoff_rfc3339 =
        (chrono::Utc::now() - chrono::Duration::days(retention_days as i64)).to_rfc3339();
    let store_rej = state.store().clone();
    let cutoff_rej = cutoff_rfc3339.clone();
    let deleted_rej = tokio::task::spawn_blocking(move || {
        store_rej.delete_ingest_rejections_older_than(&cutoff_rej)
    })
    .await
    .map_err(|e| format!("spawn_blocking(ingest_rejections): {e}"))?
    .map_err(|e| format!("delete_ingest_rejections_older_than: {e}"))?;
    if deleted_rej > 0 {
        tracing::info!(
            deleted = deleted_rej,
            cutoff = %cutoff_rfc3339,
            retention_days,
            "telemetry_cleanup swept old ingest_rejection rows"
        );
    }

    // 资源包安装留痕表同 retention 清理。installed_at 为 RFC3339（见 record_pack_install），
    // 复用上方 RFC3339 cutoff。此前该表无任何 retention，只增不删（单调膨胀）。
    let store_pack = state.store().clone();
    let cutoff_pack = cutoff_rfc3339.clone();
    let deleted_pack = tokio::task::spawn_blocking(move || {
        store_pack.delete_pack_installs_older_than(&cutoff_pack)
    })
    .await
    .map_err(|e| format!("spawn_blocking(pack_installs): {e}"))?
    .map_err(|e| format!("delete_pack_installs_older_than: {e}"))?;
    if deleted_pack > 0 {
        tracing::info!(
            deleted = deleted_pack,
            cutoff = %cutoff_rfc3339,
            retention_days,
            "telemetry_cleanup swept old resource_pack_install_log rows"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_cutoff_matches_sqlite_datetime_now_format() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-15T08:53:07Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let s = retention_cutoff_str(now, 90);
        // 与 SQLite datetime('now') 同格式（空格分隔、无 'T'、无时区后缀），90 天前 = 2026-03-17。
        assert_eq!(s, "2026-03-17 08:53:07");
        assert!(!s.contains('T'), "cutoff 不得含 'T'（须与 datetime('now') 空格格式一致）");
        assert!(
            !s.contains('+') && !s.ends_with('Z'),
            "cutoff 不得含时区后缀，否则字典序比较失配"
        );
    }
}
