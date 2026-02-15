use crate::amas::metrics::{MetricsRegistry, MetricsSnapshot};
use crate::amas::types::AlgorithmId;
use crate::store::{keys, Store};

const ALL_ALGORITHM_IDS: &[AlgorithmId] = &[
    AlgorithmId::Heuristic,
    AlgorithmId::Ige,
    AlgorithmId::Swd,
    AlgorithmId::Ensemble,
    AlgorithmId::Mdm,
    AlgorithmId::Mastery,
];

pub fn flush_metrics(
    registry: &MetricsRegistry,
    store: &Store,
) -> Result<(), crate::store::StoreError> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let snapshot = registry.snapshot_and_reset();

    let mut batch_entries: Vec<(String, serde_json::Value)> = Vec::with_capacity(snapshot.len());

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

        let key = keys::metrics_daily_key(&today, algo_id)?;
        let value =
            serde_json::to_value(merged).map_err(crate::store::StoreError::Serialization)?;
        batch_entries.push((key, value));
    }

    store.batch_upsert_metrics_daily(&batch_entries)?;

    tracing::debug!(algorithms = snapshot.len(), "Metrics flushed");
    Ok(())
}

pub fn restore_from_store(
    registry: &MetricsRegistry,
    store: &Store,
) -> Result<(), crate::store::StoreError> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    for algo in ALL_ALGORITHM_IDS {
        let algo_id = algo.as_str();
        if let Some(val) = store.get_metrics_daily(&today, algo_id)? {
            if let Ok(snapshot) = serde_json::from_value::<MetricsSnapshot>(val) {
                registry.restore(algo_id, &snapshot);
            }
        }
    }

    tracing::debug!("Metrics restored from store for {}", today);
    Ok(())
}
