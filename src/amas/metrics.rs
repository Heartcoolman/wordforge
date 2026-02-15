use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::amas::types::AlgorithmId;

const LATENCY_BUCKETS: [u64; 6] = [100, 500, 1_000, 5_000, 10_000, u64::MAX];

pub struct AlgorithmMetrics {
    pub call_count: AtomicU64,
    pub total_latency_us: AtomicU64,
    pub error_count: AtomicU64,
    pub last_called_at: AtomicI64,
    latency_buckets: [AtomicU64; 6],
}

impl Default for AlgorithmMetrics {
    fn default() -> Self {
        Self {
            call_count: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            last_called_at: AtomicI64::new(0),
            latency_buckets: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        }
    }
}

impl AlgorithmMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_latency_bucket(&self, latency_us: u64) {
        for (i, &threshold) in LATENCY_BUCKETS.iter().enumerate() {
            if latency_us <= threshold {
                self.latency_buckets[i].fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
    }

    pub fn get_percentiles(&self) -> (f64, f64, f64) {
        let counts: Vec<u64> = self
            .latency_buckets
            .iter()
            .map(|b| b.load(Ordering::Relaxed))
            .collect();
        let total: u64 = counts.iter().sum();
        if total == 0 {
            return (0.0, 0.0, 0.0);
        }

        let bucket_midpoints: [f64; 6] = [50.0, 300.0, 750.0, 3000.0, 7500.0, 15000.0];

        let percentile = |pct: f64| -> f64 {
            let target = (pct / 100.0 * total as f64).ceil() as u64;
            let mut cumulative = 0u64;
            for (i, &count) in counts.iter().enumerate() {
                cumulative += count;
                if cumulative >= target {
                    return bucket_midpoints[i];
                }
            }
            bucket_midpoints[5]
        };

        (percentile(50.0), percentile(95.0), percentile(99.0))
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
            metric.record_latency_bucket(latency_us);
            metric.last_called_at.store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
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
                for bucket in &metric.latency_buckets {
                    bucket.swap(0, Ordering::Relaxed);
                }
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
            for bucket in &metric.latency_buckets {
                bucket.store(0, Ordering::Relaxed);
            }
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
