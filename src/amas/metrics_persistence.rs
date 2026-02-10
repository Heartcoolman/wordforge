use crate::amas::metrics::{MetricsRegistry, MetricsSnapshot};
use crate::store::Store;

pub fn flush_metrics(
    registry: &MetricsRegistry,
    store: &Store,
) -> Result<(), crate::store::StoreError> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let snapshot = registry.snapshot_and_reset();

    for (algo_id, metrics) in &snapshot {
        let merged = match store.get_metrics_daily(&today, algo_id)? {
            Some(existing_val) => {
                let mut existing: MetricsSnapshot =
                    serde_json::from_value(existing_val).unwrap_or(MetricsSnapshot {
                        call_count: 0,
                        total_latency_us: 0,
                        error_count: 0,
                    });
                existing.call_count += metrics.call_count;
                existing.total_latency_us += metrics.total_latency_us;
                existing.error_count += metrics.error_count;
                existing
            }
            None => metrics.clone(),
        };

        let value = serde_json::to_value(merged)
            .map_err(|e| crate::store::StoreError::Serialization(e))?;
        store.upsert_metrics_daily(&today, algo_id, &value)?;
    }

    tracing::debug!(algorithms = snapshot.len(), "Metrics flushed");
    Ok(())
}

pub fn restore_from_store(
    _registry: &MetricsRegistry,
    _store: &Store,
) -> Result<(), crate::store::StoreError> {
    // TODO: restore today's metrics from store to registry on startup
    Ok(())
}
