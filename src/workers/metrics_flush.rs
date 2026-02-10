use crate::amas::metrics::MetricsRegistry;
use crate::store::Store;

pub async fn run(registry: &MetricsRegistry, store: &Store) {
    tracing::debug!("metrics_flush: start");
    match crate::amas::metrics_persistence::flush_metrics(registry, store) {
        Ok(()) => tracing::debug!("metrics_flush: done"),
        Err(e) => tracing::error!(error=%e, "metrics_flush failed"),
    }
}
