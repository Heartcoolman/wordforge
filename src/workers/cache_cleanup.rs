//! B68: AMAS cache cleanup (every 10 minutes)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("AMAS cache cleanup worker tick");

    // 与 monitoring_retention 对齐 30 天保留窗口：cache_cleanup 每 10 分钟跑一次，
    // 若用更短的 7 天窗口会提前删空 engine_monitoring_events，架空 30 天保留承诺，
    // 令 admin AMAS 仪表盘 8-30 天回溯查询拿到空数据。统一窗口消除双权威。
    let cutoff = (chrono::Utc::now()
        - chrono::Duration::days(crate::workers::monitoring_retention::RETENTION_DAYS))
    .to_rfc3339();
    let store = store.clone();
    match crate::blocking::run_blocking("worker.cache_cleanup", move || {
        store.cleanup_old_monitoring_events(&cutoff)
    })
    .await
    {
        Ok(Ok(removed)) if removed > 0 => {
            tracing::info!(removed, "Cache cleanup: removed old monitoring events");
        }
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "Cache cleanup: failed to clean monitoring events");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Cache cleanup task failed");
        }
        _ => {}
    }
}
