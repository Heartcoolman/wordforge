//! B68: AMAS cache cleanup (every 10 minutes)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("AMAS cache cleanup worker tick");

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
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
