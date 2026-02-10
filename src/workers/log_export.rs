//! B76: Log export (hourly)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("Log export worker tick");

    // Export recent monitoring events to metrics daily
    let events = match store.get_recent_monitoring_events(100) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to get monitoring events for export");
            return;
        }
    };

    if events.is_empty() {
        return;
    }

    let date = chrono::Utc::now().format("%Y-%m-%d-%H").to_string();
    let export = serde_json::json!({
        "exportDate": date,
        "eventCount": events.len(),
        "exported": true,
    });

    if let Err(e) = store.upsert_metrics_daily(&date, "log_export", &export) {
        tracing::warn!(error = %e, "Failed to store log export metrics");
    }

    tracing::debug!(count = events.len(), "Log export complete");
}
