//! B68: AMAS cache cleanup (every 10 minutes)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("AMAS cache cleanup worker tick");

    // Clean up expired monitoring events (older than 7 days)
    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let cutoff_ms = cutoff.timestamp_millis();
    let mut removed = 0u32;

    let mut to_remove = Vec::new();
    for item in store.engine_monitoring_events.iter() {
        let (k, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };

        if let Ok(event) = serde_json::from_slice::<serde_json::Value>(&v) {
            let event_ts = event
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp_millis())
                .unwrap_or(0);

            if event_ts < cutoff_ms {
                to_remove.push(k.to_vec());
            }
        }
    }

    for key in to_remove {
        if let Ok(()) = store
            .engine_monitoring_events
            .remove(key)
            .map(|_| ())
        {
            removed += 1;
        }
    }

    if removed > 0 {
        tracing::info!(removed, "Cache cleanup: removed old monitoring events");
    }
}
