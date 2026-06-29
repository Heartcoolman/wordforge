//! 选词拒绝原因指标：热路径只做内存原子累加，metrics_flush worker 周期 drain 增量
//! upsert 进 rejections_rollup 表。
//!
//! 设计要点（A4）：
//! - 仅按固定小枚举 `RejectionReason` 聚合，绝不含 word_id / user_id（零基数爆炸、零 PII）。
//! - `record` / `record_n` 仅做 `AtomicU64::fetch_add(Relaxed)`，零 alloc / 零 await / 无锁，
//!   可安全嵌入 word_selector 热路径。
//! - 仿 `crate::amas::metrics::MetricsRegistry` 的 snapshot_and_reset / merge_snapshot 语义：
//!   flush 取走计数后清零；落库失败由 worker 原样加回，避免该区间计数永久丢失。

use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;

/// 选词拒绝原因。fieldless 枚举，判别式 0..N 即数组下标（见 `index`）。
/// 新增变体须同步追加到 `ALL`、`as_str`、`from_str` 并保持判别式顺序与 `ALL` 一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionReason {
    /// 新词候选在 words 表查无对应行（word_selector continue 跳过）。
    WordNotFound,
    /// 复习词召回概率 >= recall_mastered_threshold，被判定已掌握而抑制（score≈0.001）。
    MasteredSuppressed,
    /// Top-K 选择阶段排序后被截断落选的候选数。
    NotTopK,
    /// 命中本 session 混淆隔离集合且 dampen<1.0，评分被惩罚。
    ConfusionDampened,
}

impl RejectionReason {
    /// 全部变体，顺序必须与判别式（0..N）一致，供注册表按下标遍历。
    pub const ALL: [RejectionReason; 4] = [
        RejectionReason::WordNotFound,
        RejectionReason::MasteredSuppressed,
        RejectionReason::NotTopK,
        RejectionReason::ConfusionDampened,
    ];

    /// 稳定字符串标识（持久化到 rejections_rollup.reason，禁止随意改名）。
    pub fn as_str(self) -> &'static str {
        match self {
            RejectionReason::WordNotFound => "word_not_found",
            RejectionReason::MasteredSuppressed => "mastered_suppressed",
            RejectionReason::NotTopK => "not_top_k",
            RejectionReason::ConfusionDampened => "confusion_dampened",
        }
    }

    /// 字符串 → 变体，供 merge_snapshot 回灌；未知字符串返回 None（丢弃，不 panic）。
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "word_not_found" => Some(RejectionReason::WordNotFound),
            "mastered_suppressed" => Some(RejectionReason::MasteredSuppressed),
            "not_top_k" => Some(RejectionReason::NotTopK),
            "confusion_dampened" => Some(RejectionReason::ConfusionDampened),
            _ => None,
        }
    }

    #[inline]
    fn index(self) -> usize {
        self as usize
    }
}

/// per-reason 原子计数注册表，按 `RejectionReason::index()` 下标寻址。
pub struct RejectionRegistry {
    counters: [AtomicU64; 4],
}

impl RejectionRegistry {
    fn new() -> Self {
        Self {
            counters: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        }
    }

    /// 热路径累加：仅 fetch_add(Relaxed)，零 alloc。
    #[inline]
    pub fn record_n(&self, reason: RejectionReason, n: u64) {
        if n == 0 {
            return;
        }
        self.counters[reason.index()].fetch_add(n, Ordering::Relaxed);
    }

    /// drain：返回非零计数 (reason_str, count) 并把对应计数器 swap 为 0。
    pub fn snapshot_and_reset(&self) -> Vec<(String, u64)> {
        RejectionReason::ALL
            .iter()
            .filter_map(|reason| {
                let v = self.counters[reason.index()].swap(0, Ordering::Relaxed);
                (v > 0).then(|| (reason.as_str().to_string(), v))
            })
            .collect()
    }

    /// flush 失败回灌：把 snapshot_and_reset 取走的计数原子加回，下次 flush 重试。
    /// 用 fetch_add（非 store），与失败窗口内新到达的 record 增量累加而不互相覆盖。
    pub fn merge_snapshot(&self, snapshot: &[(String, u64)]) {
        for (reason, n) in snapshot {
            if let Some(r) = RejectionReason::from_str(reason) {
                self.counters[r.index()].fetch_add(*n, Ordering::Relaxed);
            }
        }
    }
}

impl Default for RejectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 全局拒绝原因注册表。热路径经 `record` / `record_n` 写入，worker 经 snapshot_and_reset 读取。
pub static REJECTION_REGISTRY: Lazy<RejectionRegistry> = Lazy::new(RejectionRegistry::new);

/// 记录单次拒绝（热路径，零 alloc）。
#[inline]
pub fn record(reason: RejectionReason) {
    REJECTION_REGISTRY.record_n(reason, 1);
}

/// 记录批量拒绝（如 Top-K 截断落选数），零 alloc。
#[inline]
pub fn record_n(reason: RejectionReason, n: u64) {
    REJECTION_REGISTRY.record_n(reason, n);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_from_str_roundtrip_for_all_reasons() {
        for r in RejectionReason::ALL {
            assert_eq!(RejectionReason::from_str(r.as_str()), Some(r));
        }
        assert_eq!(RejectionReason::from_str("nonexistent"), None);
    }

    #[test]
    fn index_matches_all_order() {
        for (i, r) in RejectionReason::ALL.iter().enumerate() {
            assert_eq!(r.index(), i, "{r:?} index must equal ALL position");
        }
    }

    #[test]
    fn record_then_snapshot_resets_to_zero() {
        let reg = RejectionRegistry::new();
        reg.record_n(RejectionReason::WordNotFound, 3);
        reg.record_n(RejectionReason::NotTopK, 5);
        reg.record_n(RejectionReason::WordNotFound, 2);

        let mut snap = reg.snapshot_and_reset();
        snap.sort();
        assert_eq!(
            snap,
            vec![
                ("not_top_k".to_string(), 5),
                ("word_not_found".to_string(), 5),
            ]
        );
        // 二次 drain 为空（已清零）
        assert!(reg.snapshot_and_reset().is_empty());
    }

    #[test]
    fn record_zero_is_noop() {
        let reg = RejectionRegistry::new();
        reg.record_n(RejectionReason::ConfusionDampened, 0);
        assert!(reg.snapshot_and_reset().is_empty());
    }

    #[test]
    fn merge_snapshot_re_adds_counts() {
        let reg = RejectionRegistry::new();
        reg.record_n(RejectionReason::MasteredSuppressed, 4);
        let snap = reg.snapshot_and_reset();
        // 模拟 flush 失败回灌
        reg.merge_snapshot(&snap);
        // 失败窗口内又来一条
        reg.record_n(RejectionReason::MasteredSuppressed, 1);
        let snap2 = reg.snapshot_and_reset();
        assert_eq!(snap2, vec![("mastered_suppressed".to_string(), 5)]);
    }

    #[test]
    fn merge_snapshot_ignores_unknown_reason() {
        let reg = RejectionRegistry::new();
        reg.merge_snapshot(&[("unknown_xyz".to_string(), 9)]);
        assert!(reg.snapshot_and_reset().is_empty());
    }
}
