//! B68: AMAS cache cleanup (every 10 minutes)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("AMAS cache cleanup worker tick");

    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();

    match store.cleanup_old_monitoring_events(&cutoff) {
        Ok(removed) if removed > 0 => {
            tracing::info!(removed, "Cache cleanup: removed old monitoring events");
        }
        Err(e) => {
            tracing::warn!(error = %e, "Cache cleanup: failed to clean monitoring events");
        }
        _ => {}
    }
}
