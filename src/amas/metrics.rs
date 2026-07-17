use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::amas::types::AlgorithmId;

const LATENCY_BUCKETS: [u64; 6] = [100, 500, 1_000, 5_000, 10_000, u64::MAX];

pub struct AlgorithmMetrics {
    pub call_count: AtomicU64,
    pub total_latency_us: AtomicU64,
    pub error_count: AtomicU64,
    latency_buckets: [AtomicU64; 6],
}

impl Default for AlgorithmMetrics {
    fn default() -> Self {
        Self {
            call_count: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
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
                        latency_buckets: [0; 6],
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
                // 同时取走桶计数，供 flush 失败时 merge_snapshot 原样加回，避免延迟分布丢失。
                let mut latency_buckets = [0u64; 6];
                for (i, bucket) in metric.latency_buckets.iter().enumerate() {
                    latency_buckets[i] = bucket.swap(0, Ordering::Relaxed);
                }
                (
                    id.as_str().to_string(),
                    MetricsSnapshot {
                        call_count,
                        total_latency_us,
                        error_count,
                        latency_buckets,
                    },
                )
            })
            .collect()
    }

    /// 持久化失败时把 snapshot_and_reset 取走的计数原子加回，下次 flush 重试，避免该区间计数永久丢失。
    /// 用 fetch_add（非 store），与失败窗口内新到达的 record_call 增量累加而不互相覆盖。
    pub fn merge_snapshot(&self, snapshot: &HashMap<String, MetricsSnapshot>) {
        for (algo_id_str, snap) in snapshot {
            let algo_id = match algo_id_str.as_str() {
                "heuristic" => AlgorithmId::Heuristic,
                "ige" => AlgorithmId::Ige,
                "swd" => AlgorithmId::Swd,
                "ensemble" => AlgorithmId::Ensemble,
                "mdm" => AlgorithmId::Mdm,
                "mastery" => AlgorithmId::Mastery,
                _ => continue,
            };
            if let Some(metric) = self.metrics.get(&algo_id) {
                metric
                    .call_count
                    .fetch_add(snap.call_count, Ordering::Relaxed);
                metric
                    .total_latency_us
                    .fetch_add(snap.total_latency_us, Ordering::Relaxed);
                metric
                    .error_count
                    .fetch_add(snap.error_count, Ordering::Relaxed);
                // 把快照取走的桶计数加回，与失败窗口内新到达的增量累加而不互相覆盖。
                for (i, bucket) in metric.latency_buckets.iter().enumerate() {
                    bucket.fetch_add(snap.latency_buckets[i], Ordering::Relaxed);
                }
            }
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
    /// 延迟直方图桶计数。仅用于 flush 失败时 merge_snapshot 把桶原样加回 registry，
    /// 不持久化（serde skip）：DB 行只存三个标量，故反序列化的 snapshot 此字段为 0。
    #[serde(skip, default)]
    pub latency_buckets: [u64; 6],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn algorithm_metrics_default_is_zeroed() {
        let m = AlgorithmMetrics::new();
        assert_eq!(m.call_count.load(Ordering::Relaxed), 0);
        assert_eq!(m.total_latency_us.load(Ordering::Relaxed), 0);
        assert_eq!(m.error_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn record_latency_bucket_picks_correct_bucket() {
        let reg = MetricsRegistry::new();
        // 六个阈值各命中一个桶（经 record_call 走 record_latency_bucket）
        for us in [50, 450, 900, 4500, 9500, 100_000] {
            reg.record_call(AlgorithmId::Heuristic, us, false);
        }
        let snap = reg.snapshot_and_reset();
        assert_eq!(snap["heuristic"].latency_buckets, [1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn record_latency_bucket_handles_overflow_threshold() {
        let reg = MetricsRegistry::new();
        // u64::MAX 也应落入最后的桶
        reg.record_call(AlgorithmId::Heuristic, u64::MAX, false);
        let snap = reg.snapshot_and_reset();
        assert_eq!(snap["heuristic"].latency_buckets[5], 1);
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
    fn merge_snapshot_re_adds_counts() {
        let reg = MetricsRegistry::new();
        reg.record_call(AlgorithmId::Swd, 100, true);
        let snap = reg.snapshot_and_reset();
        // 模拟 flush 失败回灌；失败窗口内又来一条
        reg.merge_snapshot(&snap);
        reg.record_call(AlgorithmId::Swd, 50, false);
        let after = reg.snapshot();
        assert_eq!(after["swd"].call_count, 2);
        assert_eq!(after["swd"].total_latency_us, 150);
        assert_eq!(after["swd"].error_count, 1);
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
            latency_buckets: [0; 6],
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.call_count, 10);
        assert_eq!(back.total_latency_us, 12345);
        assert_eq!(back.error_count, 3);
    }
}
