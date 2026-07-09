//! T1.3 采纳门：把两臂原始计数 + 实验预注册阈值，评估为带 CI/p 值的逐指标对比 + 采纳建议。
//!
//! 纪律（墨墨 MMX-6 教训）：离线赢 ≠ 真实赢。采纳须同时满足：
//!   (a) 两臂在 primary 上样本量均 ≥ min_sample；
//!   (b) primary 两臂检验 p<alpha、方向为"更好"、且效应 ≥ MDE；
//!   (c) 无 guardrail 指标向"更差"方向显著移动。
//! 任一不满足 → 不采纳，即便离线 benchmark 赢。

use serde::Serialize;

use super::stats::{self};
use crate::store::operations::amas_experiment::{
    ArmRaw, ExperimentRawMetrics, MeanRaw, ProportionRaw,
};
use crate::store::operations::amas_experiment::ExperimentRow;

/// 北极星指标元信息：名 + 是否"越大越好"。reviews_per_day 越小越好（workload 效率）。
const METRICS: &[(&str, bool)] = &[
    ("day7_retention", true),
    ("day30_retention", true),
    ("session_completion", true),
    ("mastered_hold", true),
    ("reviews_per_day", false),
];

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricPoint {
    pub value: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub n: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricComparison {
    pub metric: String,
    /// "proportion" | "mean"
    pub kind: String,
    pub higher_is_better: bool,
    pub canary: MetricPoint,
    pub baseline: MetricPoint,
    /// canary − baseline
    pub diff: f64,
    pub p_value: Option<f64>,
    pub significant: bool,
    /// 显著且朝"更差"方向（guardrail 恶化判定用）。
    pub regressed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentVerdict {
    pub experiment_id: String,
    pub primary_metric: String,
    pub alpha: f64,
    pub mde: f64,
    pub min_sample: u64,
    pub metrics: Vec<MetricComparison>,
    /// 两臂 primary 样本量均达标。
    pub sample_ok: bool,
    pub primary_significant: bool,
    pub primary_positive: bool,
    pub primary_meets_mde: bool,
    /// 向"更差"方向显著移动的 guardrail 指标名。
    pub guardrail_regressions: Vec<String>,
    pub adopt_recommended: bool,
    /// 人类可读的判定理由（首条不满足项）。
    pub reason: String,
}

fn point(value: f64, ci_low: f64, ci_high: f64, n: u64) -> MetricPoint {
    MetricPoint {
        value,
        ci_low,
        ci_high,
        n,
    }
}

fn compare_proportion(
    name: &str,
    higher_is_better: bool,
    c: ProportionRaw,
    b: ProportionRaw,
    z: f64,
    alpha: f64,
) -> MetricComparison {
    let cc = stats::wilson_ci(c.successes, c.n, z);
    let bb = stats::wilson_ci(b.successes, b.n, z);
    let test = stats::two_proportion_z(c.successes, c.n, b.successes, b.n);
    let p_value = test.map(|t| t.p_value);
    let diff = cc.p - bb.p;
    let significant = p_value.map(|pv| pv < alpha).unwrap_or(false);
    // 朝"更差"方向：越大越好的指标显著下降，或越小越好的指标显著上升。
    let regressed = significant && ((higher_is_better && diff < 0.0) || (!higher_is_better && diff > 0.0));
    MetricComparison {
        metric: name.to_string(),
        kind: "proportion".to_string(),
        higher_is_better,
        canary: point(cc.p, cc.low, cc.high, cc.n),
        baseline: point(bb.p, bb.low, bb.high, bb.n),
        diff,
        p_value,
        significant,
        regressed,
    }
}

fn compare_mean(
    name: &str,
    higher_is_better: bool,
    c: MeanRaw,
    b: MeanRaw,
    z: f64,
    alpha: f64,
) -> MetricComparison {
    let cc = stats::mean_ci_normal(c.mean, c.var, c.n, z);
    let bb = stats::mean_ci_normal(b.mean, b.var, b.n, z);
    let test = stats::welch_t(c.mean, c.var, c.n, b.mean, b.var, b.n);
    let p_value = test.map(|t| t.p_value);
    let diff = c.mean - b.mean;
    let significant = p_value.map(|pv| pv < alpha).unwrap_or(false);
    let regressed = significant && ((higher_is_better && diff < 0.0) || (!higher_is_better && diff > 0.0));
    MetricComparison {
        metric: name.to_string(),
        kind: "mean".to_string(),
        higher_is_better,
        canary: point(cc.mean, cc.low, cc.high, cc.n),
        baseline: point(bb.mean, bb.low, bb.high, bb.n),
        diff,
        p_value,
        significant,
        regressed,
    }
}

fn arm_proportion(arm: &ArmRaw, metric: &str) -> ProportionRaw {
    match metric {
        "day7_retention" => arm.day7_retention,
        "day30_retention" => arm.day30_retention,
        "session_completion" => arm.session_completion,
        "mastered_hold" => arm.mastered_hold,
        _ => ProportionRaw::default(),
    }
}

/// 评估实验：逐指标 CI/检验 + 采纳门。`z` 由 alpha 推。
pub fn evaluate(exp: &ExperimentRow, raw: &ExperimentRawMetrics) -> ExperimentVerdict {
    let alpha = exp.alpha;
    let z = stats::z_for_alpha_two_sided(alpha);

    let metrics: Vec<MetricComparison> = METRICS
        .iter()
        .map(|&(name, higher)| {
            if name == "reviews_per_day" {
                compare_mean(
                    name,
                    higher,
                    raw.canary.reviews_per_day,
                    raw.baseline.reviews_per_day,
                    z,
                    alpha,
                )
            } else {
                compare_proportion(
                    name,
                    higher,
                    arm_proportion(&raw.canary, name),
                    arm_proportion(&raw.baseline, name),
                    z,
                    alpha,
                )
            }
        })
        .collect();

    let primary = metrics
        .iter()
        .find(|m| m.metric == exp.primary_metric)
        .cloned();

    let (sample_ok, primary_significant, primary_positive, primary_meets_mde) = match &primary {
        Some(p) => {
            let sample_ok = p.canary.n >= exp.min_sample && p.baseline.n >= exp.min_sample;
            let positive = if p.higher_is_better {
                p.diff > 0.0
            } else {
                p.diff < 0.0
            };
            // 相对效应 ≥ MDE（朝更好方向）。baseline 为 0 时退化为不达标（防除零）。
            let meets_mde = if p.baseline.value.abs() < f64::EPSILON {
                false
            } else {
                let rel = p.diff / p.baseline.value;
                if p.higher_is_better {
                    rel >= exp.mde
                } else {
                    -rel >= exp.mde
                }
            };
            (sample_ok, p.significant, positive, meets_mde)
        }
        None => (false, false, false, false),
    };

    // guardrail 否决叠加自身样本量门槛：小 n 上虚假显著的 guardrail 会误否决好实验
    // （方向为过度保守）；两臂均达 min_sample 才认可其 regressed 证据。
    let guardrail_regressions: Vec<String> = metrics
        .iter()
        .filter(|m| {
            m.metric != exp.primary_metric
                && m.regressed
                && m.canary.n >= exp.min_sample
                && m.baseline.n >= exp.min_sample
        })
        .map(|m| m.metric.clone())
        .collect();

    let adopt_recommended = sample_ok
        && primary_significant
        && primary_positive
        && primary_meets_mde
        && guardrail_regressions.is_empty();

    let reason = if primary.is_none() {
        format!("primary 指标 {} 不在已知北极星集合内", exp.primary_metric)
    } else if !sample_ok {
        "样本量未达 min_sample（任一臂不足）".to_string()
    } else if !primary_significant {
        "primary 指标两臂差异未达显著（p≥alpha）".to_string()
    } else if !primary_positive {
        "primary 指标方向不是更好".to_string()
    } else if !primary_meets_mde {
        "primary 指标效应未达 MDE".to_string()
    } else if !guardrail_regressions.is_empty() {
        format!("guardrail 恶化：{}", guardrail_regressions.join(", "))
    } else {
        "满足采纳门（primary 显著为正达 MDE 且无 guardrail 恶化）".to_string()
    };

    ExperimentVerdict {
        experiment_id: exp.experiment_id.clone(),
        primary_metric: exp.primary_metric.clone(),
        alpha,
        mde: exp.mde,
        min_sample: exp.min_sample,
        metrics,
        sample_ok,
        primary_significant,
        primary_positive,
        primary_meets_mde,
        guardrail_regressions,
        adopt_recommended,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp_row(primary: &str, min_sample: u64, mde: f64) -> ExperimentRow {
        ExperimentRow {
            experiment_id: "e".into(),
            suggestion_id: None,
            canary_version_hash: "vc".into(),
            baseline_version_hash: "vb".into(),
            canary_cohort_lo: 0,
            canary_cohort_hi: 50,
            primary_metric: primary.into(),
            min_sample,
            alpha: 0.05,
            power: 0.8,
            mde,
            offline_delta: None,
            status: "running".into(),
            registered_at: "t".into(),
            concluded_at: None,
            notes: None,
        }
    }

    fn arm(d7: ProportionRaw, rpd: MeanRaw) -> ArmRaw {
        ArmRaw {
            arm: "x".into(),
            enrolled: d7.n,
            day7_retention: d7,
            day30_retention: ProportionRaw::default(),
            reviews_per_day: rpd,
            session_completion: ProportionRaw::default(),
            mastered_hold: ProportionRaw::default(),
        }
    }

    #[test]
    fn adopt_when_primary_wins_and_no_guardrail_regression() {
        // canary d7=600/1000=0.60 vs baseline 500/1000=0.50，相对 +20% > MDE 0.05，显著。
        let raw = ExperimentRawMetrics {
            experiment_id: "e".into(),
            canary: arm(
                ProportionRaw { successes: 600, n: 1000 },
                MeanRaw { mean: 10.0, var: 4.0, n: 500 },
            ),
            baseline: arm(
                ProportionRaw { successes: 500, n: 1000 },
                MeanRaw { mean: 10.0, var: 4.0, n: 500 },
            ),
        };
        let v = evaluate(&exp_row("day7_retention", 100, 0.05), &raw);
        assert!(v.sample_ok);
        assert!(v.primary_significant, "p={:?}", v.metrics);
        assert!(v.primary_positive);
        assert!(v.primary_meets_mde);
        assert!(v.guardrail_regressions.is_empty());
        assert!(v.adopt_recommended);
    }

    #[test]
    fn reject_when_sample_too_small() {
        let raw = ExperimentRawMetrics {
            experiment_id: "e".into(),
            canary: arm(ProportionRaw { successes: 6, n: 10 }, MeanRaw::default()),
            baseline: arm(ProportionRaw { successes: 5, n: 10 }, MeanRaw::default()),
        };
        let v = evaluate(&exp_row("day7_retention", 100, 0.05), &raw);
        assert!(!v.sample_ok);
        assert!(!v.adopt_recommended);
    }

    #[test]
    fn reject_when_guardrail_reviews_per_day_spikes() {
        // primary 留存赢，但 reviews_per_day 显著飙升（越小越好 → 恶化）。
        let raw = ExperimentRawMetrics {
            experiment_id: "e".into(),
            canary: arm(
                ProportionRaw { successes: 600, n: 1000 },
                MeanRaw { mean: 30.0, var: 4.0, n: 500 },
            ),
            baseline: arm(
                ProportionRaw { successes: 500, n: 1000 },
                MeanRaw { mean: 10.0, var: 4.0, n: 500 },
            ),
        };
        let v = evaluate(&exp_row("day7_retention", 100, 0.05), &raw);
        assert!(v.primary_significant && v.primary_positive);
        assert!(v.guardrail_regressions.contains(&"reviews_per_day".to_string()));
        assert!(!v.adopt_recommended, "guardrail 恶化必须否决采纳");
    }

    #[test]
    fn aa_identical_arms_show_no_bias() {
        // A/A 自检：两臂完全相同 → diff=0、p≈1、不显著、不采纳。确认管线无系统性桶偏差。
        let raw = ExperimentRawMetrics {
            experiment_id: "e".into(),
            canary: arm(
                ProportionRaw { successes: 500, n: 1000 },
                MeanRaw { mean: 12.0, var: 9.0, n: 800 },
            ),
            baseline: arm(
                ProportionRaw { successes: 500, n: 1000 },
                MeanRaw { mean: 12.0, var: 9.0, n: 800 },
            ),
        };
        let v = evaluate(&exp_row("day7_retention", 100, 0.05), &raw);
        let primary = v.metrics.iter().find(|m| m.metric == "day7_retention").unwrap();
        assert!(primary.diff.abs() < 1e-12);
        assert!(primary.p_value.unwrap() > 0.99);
        assert!(!v.primary_significant);
        assert!(v.guardrail_regressions.is_empty());
        assert!(!v.adopt_recommended);
    }

    #[test]
    fn reject_when_offline_wins_but_real_flat() {
        // 两臂 primary 几乎相同（离线或可能赢，但真实不显著）→ 不采纳。墨墨铁律。
        let raw = ExperimentRawMetrics {
            experiment_id: "e".into(),
            canary: arm(ProportionRaw { successes: 505, n: 1000 }, MeanRaw::default()),
            baseline: arm(ProportionRaw { successes: 500, n: 1000 }, MeanRaw::default()),
        };
        let v = evaluate(&exp_row("day7_retention", 100, 0.05), &raw);
        assert!(!v.primary_significant);
        assert!(!v.adopt_recommended);
    }
}
