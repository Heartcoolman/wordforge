use crate::amas::metrics::MetricsRegistry;
use crate::store::Store;

pub async fn run(registry: &MetricsRegistry, store: &Store) {
    tracing::debug!("metrics_flush: start");
    let snapshot = registry.snapshot_and_reset();
    let store = store.clone();
    match crate::blocking::run_blocking("worker.metrics_flush", move || {
        crate::amas::metrics_persistence::flush_metrics_snapshot(snapshot, &store)
    })
    .await
    {
        Ok(Ok(())) => tracing::debug!("metrics_flush: done"),
        Ok(Err(e)) => tracing::error!(error=%e, "metrics_flush failed"),
        Err(e) => tracing::error!(error=%e, "metrics_flush task failed"),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::amas::types::AlgorithmId;

    #[tokio::test]
    async fn metrics_flush_persists_snapshot_via_blocking_task() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(
            dir.path().join("metrics-flush.db").to_str().unwrap(),
            5000,
            2,
        )
        .expect("open store");
        store.run_migrations().expect("run migrations");

        let registry = MetricsRegistry::new();
        registry.record_call(AlgorithmId::Heuristic, 120, false);
        registry.record_call(AlgorithmId::Heuristic, 80, true);

        run(&registry, &store).await;

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let persisted = store
            .get_metrics_daily(&today, "heuristic")
            .expect("read flushed metric")
            .expect("metric should exist");

        assert_eq!(persisted["call_count"], 2);
        assert_eq!(persisted["error_count"], 1);
        assert_eq!(persisted["total_latency_us"], 200);
    }
}
