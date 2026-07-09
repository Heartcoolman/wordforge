use crate::amas::metrics::{MetricsRegistry, MetricsSnapshot};
use crate::store::Store;

/// 同步 drain+落库+失败回灌。供优雅停机最终落盘等无 async 上下文的调用点；
/// cron 周期 flush 走 workers::metrics_flush::run（blocking 任务 + admin 告警）。
pub fn flush_metrics(
    registry: &MetricsRegistry,
    store: &Store,
) -> Result<(), crate::store::StoreError> {
    let snapshot = registry.snapshot_and_reset();
    // 落库失败时把取走的计数加回 registry，避免该区间计数随 reset 永久丢失（reset-before-confirm）。
    if let Err(e) = flush_metrics_snapshot(snapshot.clone(), store) {
        registry.merge_snapshot(&snapshot);
        return Err(e);
    }
    Ok(())
}

pub fn flush_metrics_snapshot(
    snapshot: std::collections::HashMap<String, MetricsSnapshot>,
    store: &Store,
) -> Result<(), crate::store::StoreError> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut batch_entries: Vec<(String, serde_json::Value)> = Vec::with_capacity(snapshot.len());

    for (algo_id, metrics) in &snapshot {
        let merged = match store.get_metrics_daily(&today, algo_id)? {
            Some(existing_val) => {
                let mut existing: MetricsSnapshot =
                    serde_json::from_value(existing_val).unwrap_or(MetricsSnapshot {
                        call_count: 0,
                        total_latency_us: 0,
                        error_count: 0,
                        latency_buckets: [0; 6],
                    });
                existing.call_count += metrics.call_count;
                existing.total_latency_us += metrics.total_latency_us;
                existing.error_count += metrics.error_count;
                existing
            }
            None => metrics.clone(),
        };

        let key = format!("{today}:{algo_id}");
        let value =
            serde_json::to_value(merged).map_err(crate::store::StoreError::Serialization)?;
        batch_entries.push((key, value));
    }

    store.batch_upsert_metrics_daily(&batch_entries)?;

    tracing::debug!(algorithms = snapshot.len(), "Metrics flushed");
    Ok(())
}

