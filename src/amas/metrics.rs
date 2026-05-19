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
            metric
                .last_called_at
                .store(chrono::Utc::now().timestamp_millis(), Ordering::Relaxed);
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
#[serde(rename_all = "camelCase")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithm_metrics_default_is_zeroed() {
        let m = AlgorithmMetrics::new();
        assert_eq!(m.call_count.load(Ordering::Relaxed), 0);
        assert_eq!(m.total_latency_us.load(Ordering::Relaxed), 0);
        assert_eq!(m.error_count.load(Ordering::Relaxed), 0);
        assert_eq!(m.last_called_at.load(Ordering::Relaxed), 0);
        let (p50, p95, p99) = m.get_percentiles();
        assert_eq!((p50, p95, p99), (0.0, 0.0, 0.0));
    }

    #[test]
    fn record_latency_bucket_picks_correct_bucket() {
        let m = AlgorithmMetrics::default();
        m.record_latency_bucket(50);
        m.record_latency_bucket(450);
        m.record_latency_bucket(900);
        m.record_latency_bucket(4500);
        m.record_latency_bucket(9500);
        m.record_latency_bucket(100_000);
        // All six buckets should have exactly one entry; percentiles must be finite.
        let (p50, p95, p99) = m.get_percentiles();
        assert!(p50 > 0.0 && p95 > 0.0 && p99 > 0.0);
        // P99 should be the highest bucket midpoint.
        assert!(p99 >= p95);
        assert!(p95 >= p50);
    }

    #[test]
    fn record_latency_bucket_handles_overflow_threshold() {
        let m = AlgorithmMetrics::default();
        // u64::MAX 也应落入最后的桶
        m.record_latency_bucket(u64::MAX);
        let (_, _, p99) = m.get_percentiles();
        assert_eq!(p99, 15000.0);
    }

    #[test]
    fn registry_record_call_updates_metrics() {
        let reg = MetricsRegistry::new();
        reg.record_call(AlgorithmId::Heuristic, 200, false);
        reg.record_call(AlgorithmId::Heuristic, 600, true);
        let snap = reg.snapshot();
        let h = snap.get("heuristic").expect("heuristic in snapshot");
        assert_eq!(h.call_count, 2);
        assert_eq!(h.error_count, 1);
        assert_eq!(h.total_latency_us, 800);
    }

    #[test]
    fn registry_snapshot_includes_all_algorithms() {
        let reg = MetricsRegistry::new();
        let snap = reg.snapshot();
        for key in ["heuristic", "ige", "swd", "ensemble", "mdm", "mastery"] {
            assert!(snap.contains_key(key), "missing key {key}");
        }
    }

    #[test]
    fn snapshot_and_reset_returns_counts_then_zeroes() {
        let reg = MetricsRegistry::new();
        reg.record_call(AlgorithmId::Ige, 100, false);
        reg.record_call(AlgorithmId::Ige, 200, false);
        let snap1 = reg.snapshot_and_reset();
        assert_eq!(snap1["ige"].call_count, 2);
        assert_eq!(snap1["ige"].total_latency_us, 300);
        let snap2 = reg.snapshot();
        assert_eq!(snap2["ige"].call_count, 0);
        assert_eq!(snap2["ige"].total_latency_us, 0);
    }

    #[test]
    fn restore_roundtrips_snapshot() {
        let reg = MetricsRegistry::new();
        let snap = MetricsSnapshot {
            call_count: 7,
            total_latency_us: 1234,
            error_count: 2,
        };
        reg.restore("swd", &snap);
        let after = reg.snapshot();
        assert_eq!(after["swd"].call_count, 7);
        assert_eq!(after["swd"].total_latency_us, 1234);
        assert_eq!(after["swd"].error_count, 2);
    }

    #[test]
    fn restore_unknown_algo_is_noop() {
        let reg = MetricsRegistry::new();
        let snap = MetricsSnapshot {
            call_count: 9,
            total_latency_us: 999,
            error_count: 0,
        };
        reg.restore("unknown_algo_xyz", &snap);
        // 不影响已知 key
        let after = reg.snapshot();
        for v in after.values() {
            assert_eq!(v.call_count, 0);
        }
    }

    #[test]
    fn restore_covers_all_known_algo_strings() {
        let reg = MetricsRegistry::new();
        for key in ["heuristic", "ige", "swd", "ensemble", "mdm", "mastery"] {
            reg.restore(
                key,
                &MetricsSnapshot {
                    call_count: 1,
                    total_latency_us: 10,
                    error_count: 0,
                },
            );
        }
        let snap = reg.snapshot();
        for key in ["heuristic", "ige", "swd", "ensemble", "mdm", "mastery"] {
            assert_eq!(snap[key].call_count, 1, "{key} not restored");
        }
    }

    #[test]
    fn reset_clears_all_counters_and_buckets() {
        let reg = MetricsRegistry::new();
        reg.record_call(AlgorithmId::Mdm, 50, false);
        reg.record_call(AlgorithmId::Mdm, 5000, true);
        reg.reset();
        let snap = reg.snapshot();
        assert_eq!(snap["mdm"].call_count, 0);
        assert_eq!(snap["mdm"].error_count, 0);
        assert_eq!(snap["mdm"].total_latency_us, 0);
    }

    #[test]
    fn record_call_on_unknown_id_is_noop() {
        // record_call 仅注册了已初始化的 6 个 AlgorithmId；任何 metrics 注册时未包含的 id
        // 会走 if let Some 失败分支。这里直接验证调用不会 panic 即可。
        // （MetricsRegistry::new 已包含全部 AlgorithmId 枚举值，因此该路径靠 unsafe 不可达。
        // 我们改用一个手动构造的、剔除 Heuristic 的 registry 来覆盖该分支。）
        let mut metrics = HashMap::new();
        metrics.insert(AlgorithmId::Ige, AlgorithmMetrics::new());
        let reg = MetricsRegistry { metrics };
        reg.record_call(AlgorithmId::Heuristic, 100, false);
        let snap = reg.snapshot();
        assert!(snap.get("heuristic").is_none());
        assert_eq!(snap["ige"].call_count, 0);
    }

    #[test]
    fn metrics_registry_default_equals_new() {
        let a = MetricsRegistry::default().snapshot();
        let b = MetricsRegistry::new().snapshot();
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn metrics_snapshot_serde_roundtrip() {
        let snap = MetricsSnapshot {
            call_count: 10,
            total_latency_us: 12345,
            error_count: 3,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.call_count, 10);
        assert_eq!(back.total_latency_us, 12345);
        assert_eq!(back.error_count, 3);
    }

    #[test]
    fn track_algorithm_macro_records_ok_path() {
        let reg = MetricsRegistry::new();
        let result: Result<i32, &str> = track_algorithm!(reg, AlgorithmId::Ensemble, Ok(42));
        assert_eq!(result.unwrap(), 42);
        let snap = reg.snapshot();
        assert_eq!(snap["ensemble"].call_count, 1);
        assert_eq!(snap["ensemble"].error_count, 0);
    }

    #[test]
    fn track_algorithm_macro_records_err_path() {
        let reg = MetricsRegistry::new();
        let result: Result<i32, &str> = track_algorithm!(reg, AlgorithmId::Ensemble, Err("nope"));
        assert!(result.is_err());
        let snap = reg.snapshot();
        assert_eq!(snap["ensemble"].call_count, 1);
        assert_eq!(snap["ensemble"].error_count, 1);
    }
}
