use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::amas::types::AlgorithmId;

pub struct AlgorithmMetrics {
    pub call_count: AtomicU64,
    pub total_latency_us: AtomicU64,
    pub error_count: AtomicU64,
    pub last_called_at: RwLock<Option<chrono::DateTime<chrono::Utc>>>,
}

impl AlgorithmMetrics {
    pub fn new() -> Self {
        Self {
            call_count: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            last_called_at: RwLock::new(None),
        }
    }
}

pub struct MetricsRegistry {
    metrics: HashMap<AlgorithmId, AlgorithmMetrics>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        let mut metrics = HashMap::new();
        for id in &[
            AlgorithmId::Heuristic,
            AlgorithmId::Ige,
            AlgorithmId::Swd,
            AlgorithmId::Ensemble,
            AlgorithmId::Mdm,
            AlgorithmId::Mastery,
        ] {
            metrics.insert(*id, AlgorithmMetrics::new());
        }
        Self { metrics }
    }

    pub fn record_call(&self, id: AlgorithmId, latency_us: u64, is_error: bool) {
        if let Some(metric) = self.metrics.get(&id) {
            metric.call_count.fetch_add(1, Ordering::Relaxed);
            metric
                .total_latency_us
                .fetch_add(latency_us, Ordering::Relaxed);
            if is_error {
                metric.error_count.fetch_add(1, Ordering::Relaxed);
            }

            if let Ok(mut guard) = metric.last_called_at.try_write() {
                *guard = Some(chrono::Utc::now());
            }
        }
    }

    pub fn snapshot(&self) -> HashMap<String, MetricsSnapshot> {
        self.metrics
            .iter()
            .map(|(id, metric)| {
                (
                    id.as_str().to_string(),
                    MetricsSnapshot {
                        call_count: metric.call_count.load(Ordering::Relaxed),
                        total_latency_us: metric.total_latency_us.load(Ordering::Relaxed),
                        error_count: metric.error_count.load(Ordering::Relaxed),
                    },
                )
            })
            .collect()
    }

    pub fn snapshot_and_reset(&self) -> HashMap<String, MetricsSnapshot> {
        self.metrics
            .iter()
            .map(|(id, metric)| {
                let call_count = metric.call_count.swap(0, Ordering::Relaxed);
                let total_latency_us = metric.total_latency_us.swap(0, Ordering::Relaxed);
                let error_count = metric.error_count.swap(0, Ordering::Relaxed);
                (
                    id.as_str().to_string(),
                    MetricsSnapshot {
                        call_count,
                        total_latency_us,
                        error_count,
                    },
                )
            })
            .collect()
    }

    /// 从持久化的快照恢复 metrics 数据（启动时调用）
    pub fn restore(&self, algo_id_str: &str, snapshot: &MetricsSnapshot) {
        let algo_id = match algo_id_str {
            "heuristic" => AlgorithmId::Heuristic,
            "ige" => AlgorithmId::Ige,
            "swd" => AlgorithmId::Swd,
            "ensemble" => AlgorithmId::Ensemble,
            "mdm" => AlgorithmId::Mdm,
            "mastery" => AlgorithmId::Mastery,
            _ => return,
        };
        if let Some(metric) = self.metrics.get(&algo_id) {
            metric
                .call_count
                .store(snapshot.call_count, Ordering::Relaxed);
            metric
                .total_latency_us
                .store(snapshot.total_latency_us, Ordering::Relaxed);
            metric
                .error_count
                .store(snapshot.error_count, Ordering::Relaxed);
        }
    }

    pub fn reset(&self) {
        for metric in self.metrics.values() {
            metric.call_count.store(0, Ordering::Relaxed);
            metric.total_latency_us.store(0, Ordering::Relaxed);
            metric.error_count.store(0, Ordering::Relaxed);
        }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub call_count: u64,
    pub total_latency_us: u64,
    pub error_count: u64,
}

#[allow(unused_macros)]
macro_rules! track_algorithm {
    ($registry:expr, $id:expr, $block:expr) => {{
        let start = std::time::Instant::now();
        let result = $block;
        let latency_us = start.elapsed().as_micros() as u64;
        let is_error = result.is_err();
        $registry.record_call($id, latency_us, is_error);
        result
    }};
}

#[allow(unused_imports)]
pub(crate) use track_algorithm;
