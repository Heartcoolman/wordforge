//! T1.3 真实留存 A/B 验证闭环：实验注册、桶归属、统计判定。
//!
//! 核心不变式：切分维度用独立 `experiment_id` + `arm`（非 config_version 内容哈希），
//! 使 no-op A/A（同配置两桶）可分；用户首次进桶时 arm/Day-0 冻结，scale/rollback 后
//! 跨桶漂移不改写已有归属，跨天留存按 enrollment 时点归因。

pub mod evaluate;
pub mod stats;

use serde::{Deserialize, Serialize};

/// 实验规划：按 primary 指标基线 + 预注册 MDE/alpha/power 反推每桶所需最小样本量与推荐 percent。
/// 决策4：percent 由后端反推（5% 对 Day-30 低频留存大概率样本不足）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentPlan {
    /// 每桶所需最小样本量（enrolled 用户数）。
    pub min_sample_per_arm: u64,
    /// 两臂合计所需样本量。
    pub total_required: u64,
    /// 给定日注册量下，达样本量所需运行天数（向上取整）。daily_signups<=0 时为 None。
    pub estimated_days: Option<u64>,
    /// 推荐 canary 占比（百分比整数）：默认两臂等分 → 50%。提示运维：5% 多半不足。
    pub recommended_percent: u32,
}

/// 比例型 primary（如 day7_retention）的样本量规划。
/// p0=基线比例，mde_rel=相对提升，alpha 双侧，power，daily_signups=日进桶用户数（两臂合计）。
pub fn plan_proportion(
    p0: f64,
    mde_rel: f64,
    alpha: f64,
    power: f64,
    daily_signups: f64,
) -> ExperimentPlan {
    let n = stats::sample_size_two_proportions(p0, mde_rel, alpha, power);
    plan_from_n(n, daily_signups)
}

/// 连续型 primary（如 reviews_per_day）的样本量规划。
pub fn plan_mean(
    sigma: f64,
    delta: f64,
    alpha: f64,
    power: f64,
    daily_signups: f64,
) -> ExperimentPlan {
    let n = stats::sample_size_two_means(sigma, delta, alpha, power);
    plan_from_n(n, daily_signups)
}

fn plan_from_n(n: u64, daily_signups: f64) -> ExperimentPlan {
    let total = n.saturating_mul(2);
    let estimated_days = if daily_signups > 0.0 && n != u64::MAX {
        Some((total as f64 / daily_signups).ceil() as u64)
    } else {
        None
    };
    ExperimentPlan {
        min_sample_per_arm: n,
        total_required: total,
        estimated_days,
        recommended_percent: 50,
    }
}

/// 实验臂。canary=灰度配置桶，baseline=对照（stable）桶。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arm {
    Canary,
    Baseline,
}

impl Arm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Arm::Canary => "canary",
            Arm::Baseline => "baseline",
        }
    }
}

/// 运行中实验在引擎侧的精简快照：供热路径按桶/实际配置定 arm，避免每事件查 DB。
#[derive(Debug, Clone)]
pub struct ActiveExperiment {
    pub experiment_id: String,
    pub canary_cohort_lo: u32,
    pub canary_cohort_hi: u32,
    /// 该实验声明的 canary 配置版本哈希。A/B 下用它与本次实际命中的 config 比对定 arm。
    pub canary_version_hash: String,
    /// A/A：canary 与 baseline 同配置。此时无实际配置差异，arm 纯按 cohort 切分。
    pub aa: bool,
}

impl ActiveExperiment {
    /// 仅按 cohort 判 arm（A/A 用）：落入 canary cohort → Canary，否则 Baseline。
    pub fn arm_for_bucket(&self, bucket: u32) -> Arm {
        if bucket >= self.canary_cohort_lo && bucket < self.canary_cohort_hi {
            Arm::Canary
        } else {
            Arm::Baseline
        }
    }

    /// 按**本次实际服务的配置**定 arm，杜绝 cohort/percent/crowd_filter 错配把 stable 结果误归
    /// canary 臂：A/A 按 cohort 切；A/B 仅当本次真正命中 canary 配置（hit==canary_version_hash）才
    /// 记 Canary，否则记 Baseline（用户实际收到 stable）。
    pub fn arm_for(&self, bucket: u32, hit_version_hash: Option<&str>) -> Arm {
        if self.aa {
            self.arm_for_bucket(bucket)
        } else if hit_version_hash == Some(self.canary_version_hash.as_str()) {
            Arm::Canary
        } else {
            Arm::Baseline
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp(aa: bool) -> ActiveExperiment {
        ActiveExperiment {
            experiment_id: "e".into(),
            canary_cohort_lo: 0,
            canary_cohort_hi: 50,
            canary_version_hash: "vc".into(),
            aa,
        }
    }

    #[test]
    fn arm_for_ab_binds_to_actual_served_config() {
        let e = exp(false); // A/B
        // 命中 canary 配置 → Canary（无论桶号）
        assert_eq!(e.arm_for(10, Some("vc")), Arm::Canary);
        // cohort 内但实际收到 stable（hit=None，如 percent/crowd_filter 收窄）→ Baseline，不误归 canary
        assert_eq!(e.arm_for(10, None), Arm::Baseline);
        // 命中的是别的版本 → Baseline
        assert_eq!(e.arm_for(10, Some("other")), Arm::Baseline);
    }

    #[test]
    fn arm_for_aa_splits_by_cohort() {
        let e = exp(true); // A/A：同配置，按 cohort 切
        assert_eq!(e.arm_for(10, None), Arm::Canary); // bucket 10 ∈ [0,50)
        assert_eq!(e.arm_for(80, None), Arm::Baseline);
    }
}
