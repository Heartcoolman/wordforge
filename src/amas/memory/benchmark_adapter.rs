use serde::{Deserialize, Serialize};

use crate::amas::config::{MemoryModelConfig, SspConfig};

use super::mdm::{
    compute_interval, compute_interval_base_days, gsp_banded_retention, gsp_schedule_active,
    gsp_schedule_days, recall_probability, update_strength_with_evidence, MdmState,
};
use super::ssp;

const DAY_MS: i64 = 86_400_000;
/// 二元 recall（1=记住）按 FSRS 二元拟合惯例映射到 Good(3)，而非 Easy(4)。
/// update_strength 的评分带为 `<=0.85 → Good(3)`、`>0.85 → Easy(4)`；
/// 取 0.7 落在 Good 带内，避免首评 stability 取 w[3]（Easy）造成约 5x 膨胀、扭曲调参与算法对比。
const SUCCESS_QUALITY: f64 = 0.7;
const FAILURE_QUALITY: f64 = 0.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkHistoryItem {
    #[serde(default)]
    pub t_history: Vec<i64>,
    #[serde(default)]
    pub r_history: Vec<u8>,
    /// Alternative: pass raw CSV strings and Rust will parse them.
    #[serde(default)]
    pub t_history_csv: Option<String>,
    #[serde(default)]
    pub r_history_csv: Option<String>,
    #[serde(default)]
    pub next_t_days: Option<f64>,
    #[serde(default)]
    pub target_retentions: Vec<f64>,
    #[serde(default = "default_interval_scale")]
    pub interval_scale: f64,
}

impl BenchmarkHistoryItem {
    /// Resolve t_history: prefer parsed array, fall back to CSV string.
    pub fn resolve_t_history(&self) -> Vec<i64> {
        if !self.t_history.is_empty() {
            return self.t_history.clone();
        }
        match &self.t_history_csv {
            Some(csv) if !csv.is_empty() => csv
                .split(',')
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect(),
            _ => vec![],
        }
    }

    /// Resolve r_history: prefer parsed array, fall back to CSV string.
    pub fn resolve_r_history(&self) -> Vec<u8> {
        if !self.r_history.is_empty() {
            return self.r_history.clone();
        }
        match &self.r_history_csv {
            Some(csv) if !csv.is_empty() => csv
                .split(',')
                .filter_map(|s| s.trim().parse::<u8>().ok())
                .collect(),
            _ => vec![],
        }
    }
}

fn default_interval_scale() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkAdapterRequest {
    pub config: MemoryModelConfig,
    pub items: Vec<BenchmarkHistoryItem>,
    /// T1.4 Cost-ADR：可选 SSP 后端配置。Some → 预计算 SSP DP，scheduled_interval_days 用
    /// optimal_interval 作 base（复刻 mastery.rs SSP 分支），并暴露 optimal_retention 曲面供
    /// Python↔Rust parity（此前 SSP 后端在对拍中零覆盖）。None → MDM 路径（bit-exact legacy）。
    #[serde(default)]
    pub ssp_config: Option<SspConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionInterval {
    pub retention: f64,
    pub interval_days: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkAdapterResult {
    pub stability: f64,
    pub difficulty: f64,
    pub review_count: u32,
    pub predicted_recall: Option<f64>,
    pub intervals: Vec<RetentionInterval>,
    /// 重放终态的 correct_streak（GSP 毕业下限判定依赖；Python↔Rust 对拍可读）。
    pub correct_streak: u32,
    /// GSP 调度策略头产出的最终调度区间（天，整数）。仅在 GSP 任一旋钮激活时为 Some，
    /// 全关时为 None（生产走旧 compute_interval 路径，无 head）。base 用 baseDesiredRetention
    /// （band>0 时由 banded retention 替换），契约 GSP_SPEC §3 全 op-order。供 Python↔Rust
    /// 区间 parity 对拍（cap/floor/fuzz/banded 同序逐位）。
    pub scheduled_interval_days: Option<i64>,
    /// T1.4 Cost-ADR：该 (stability, difficulty) 状态的最优目标保持率 R（状态相关 DR 曲面）。
    /// 仅在请求带 ssp_config 时为 Some，供 Python↔Rust 对拍 DR 曲面。None=无 SSP 后端。
    #[serde(default)]
    pub optimal_retention: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkAdapterResponse {
    pub items: Vec<BenchmarkAdapterResult>,
}

pub fn evaluate_batch(
    request: BenchmarkAdapterRequest,
) -> Result<BenchmarkAdapterResponse, String> {
    let mut results = Vec::with_capacity(request.items.len());

    // T1.4：带 ssp_config 时预计算 SSP DP 一次（含 optimal_r 曲面），供下方 SSP 后端 base 区间 +
    // DR 曲面 parity。与 engine.rs 构造同序（dual_grid 二选一 + 保留 optimal_r）。
    let ssp_policy = request.ssp_config.as_ref().map(|sc| {
        let pre = ssp::precompute(sc, &request.config);
        if pre.dual_grid {
            ssp::SspPolicy::from_tables_with_bins(pre.tables, pre.stability_list)
        } else {
            ssp::SspPolicy::from_tables(pre.tables, sc)
        }
        .with_retention_tables(pre.optimal_r)
    });

    for item in request.items {
        let (state, correct_streak) = replay_history_with_streak(&item, &request.config)?;
        let now_ms = state.last_review_at.unwrap_or(0);
        let predicted_recall = item.next_t_days.map(|days| {
            recall_probability(
                &state,
                now_ms + (days * DAY_MS as f64) as i64,
                &request.config,
            )
        });
        let intervals = item
            .target_retentions
            .iter()
            .map(|&retention| {
                let seconds =
                    compute_interval(&state, retention, item.interval_scale, &request.config);
                RetentionInterval {
                    retention,
                    interval_days: seconds as f64 / 86_400.0,
                }
            })
            .collect();

        // GSP 调度策略头产出的最终调度区间（仅 GSP 激活时；契约 GSP_SPEC §3 全 op-order）。
        // base 用 baseDesiredRetention（band>0 时由 banded retention 替换），与 mastery.rs 同序。
        let scheduled_interval_days = if gsp_schedule_active(&request.config) {
            let target_recall = gsp_banded_retention(&state, &request.config)
                .unwrap_or(request.config.base_desired_retention);
            // T1.4：SSP 后端时 base 取 policy 最优天（min cap → ceil → max(1)），与 mastery.rs SSP 分支
            // 逐位同序；否则走 MDM banded base。GSP head（cap/floor/fuzz）两后端统一收口。
            let base_days_int = if let Some(policy) = &ssp_policy {
                let optimal_days = policy.optimal_interval(state.stability, state.difficulty);
                (optimal_days.min(request.config.max_interval_days).ceil() as i64).max(1)
            } else {
                compute_interval_base_days(&state, target_recall, &request.config)
            };
            Some(gsp_schedule_days(
                base_days_int,
                item.interval_scale,
                correct_streak,
                &state,
                &request.config,
            ))
        } else {
            None
        };

        // T1.4：暴露状态相关 DR 曲面值（仅 SSP 后端）供 Python↔Rust 对拍。
        let optimal_retention = ssp_policy
            .as_ref()
            .and_then(|p| p.optimal_retention(state.stability, state.difficulty));

        results.push(BenchmarkAdapterResult {
            stability: state.stability,
            difficulty: state.difficulty,
            review_count: state.review_count,
            predicted_recall,
            intervals,
            correct_streak,
            scheduled_interval_days,
            optimal_retention,
        });
    }

    Ok(BenchmarkAdapterResponse { items: results })
}

/// 仅返回终态（向后兼容入口；丢弃 streak）。
pub fn replay_history(
    item: &BenchmarkHistoryItem,
    config: &MemoryModelConfig,
) -> Result<MdmState, String> {
    replay_history_with_streak(item, config).map(|(state, _)| state)
}

/// 重放历史并返回 (终态, 终态 correct_streak)。streak 供 GSP 毕业下限判定。
pub fn replay_history_with_streak(
    item: &BenchmarkHistoryItem,
    config: &MemoryModelConfig,
) -> Result<(MdmState, u32), String> {
    let t_history = item.resolve_t_history();
    let r_history = item.resolve_r_history();

    if t_history.len() != r_history.len() {
        return Err(format!(
            "history length mismatch: t_history={} r_history={}",
            t_history.len(),
            r_history.len()
        ));
    }

    let mut state = MdmState::default();
    let mut now_ms = 0i64;
    // 忠实重放 mastery.rs:69-99 的连击动态 alpha（先推进连击、失败清零、首评 gap 恒过）
    // 与双腿信任调度证据（advance-before-update：streak=记账后连击，lapses 含本次失败）。
    // interval_scale 钉死 1.0：生产 adjusted_interval_scale ~0.95-1.08，base alpha 偏差 ≤8%，
    // 耦合会破坏 Rust↔Python 对拍可测性，作为已接受的残余保真缺口记录。
    let mut streak: u32 = 0;
    let mut lapses: u32 = 0;

    for (&delta_t, &result) in t_history.iter().zip(r_history.iter()) {
        if delta_t < 0 {
            return Err(format!("delta_t must be >= 0, got {delta_t}"));
        }
        now_ms += delta_t * DAY_MS;
        let quality = if result == 0 {
            FAILURE_QUALITY
        } else {
            SUCCESS_QUALITY
        };
        if result != 0 {
            let gap_ok = state.review_count == 0 || delta_t * DAY_MS >= config.streak_min_gap_ms;
            if gap_ok {
                streak += 1;
            }
        } else {
            streak = 0;
            // 累计 lapse 在更新前自增（生产 lapses = total_attempts - total_correct 含本次失败）
            lapses += 1;
        }
        let base_alpha = (1.0 * config.alpha_scale).clamp(config.alpha_min, config.alpha_max);
        let streak_bonus = 1.0 + (streak.min(5) as f64) * 0.1;
        let alpha = (base_alpha * streak_bonus).clamp(config.alpha_min, config.alpha_max);
        update_strength_with_evidence(&mut state, quality, alpha, streak, lapses, now_ms, config);
    }

    Ok((state, streak))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_batch_matches_direct_recall_and_interval() {
        let config = MemoryModelConfig::default();
        let item = BenchmarkHistoryItem {
            t_history: vec![0, 1, 3, 7],
            r_history: vec![1, 1, 0, 1],
            t_history_csv: None,
            r_history_csv: None,
            next_t_days: Some(2.0),
            target_retentions: vec![0.8, 0.85, 0.9],
            interval_scale: 1.0,
        };

        let state = replay_history(&item, &config).expect("state");
        let now_ms = state.last_review_at.unwrap();
        let expected_recall = recall_probability(&state, now_ms + 2 * DAY_MS, &config);
        let expected_interval = compute_interval(&state, 0.85, 1.0, &config) as f64 / 86_400.0;

        let response = evaluate_batch(BenchmarkAdapterRequest {
            config,
            items: vec![item],
            ssp_config: None,
        })
        .expect("response");
        let scored = &response.items[0];

        assert_eq!(scored.review_count, state.review_count);
        assert!((scored.stability - state.stability).abs() < 1e-9);
        assert!((scored.predicted_recall.expect("pred") - expected_recall).abs() < 1e-9);
        let interval = scored
            .intervals
            .iter()
            .find(|entry| (entry.retention - 0.85).abs() < 1e-9)
            .expect("interval");
        assert!((interval.interval_days - expected_interval).abs() < 1e-9);
    }

    #[test]
    fn replay_history_rejects_length_mismatch() {
        let config = MemoryModelConfig::default();
        let item = BenchmarkHistoryItem {
            t_history: vec![0, 1],
            r_history: vec![1],
            t_history_csv: None,
            r_history_csv: None,
            next_t_days: None,
            target_retentions: vec![],
            interval_scale: 1.0,
        };
        let err = replay_history(&item, &config).expect_err("should fail");
        assert!(err.contains("history length mismatch"));
    }

    #[test]
    fn ssp_backend_exposes_dr_surface_and_drives_interval() {
        // T1.4：带 ssp_config 时，scheduled_interval 走 SSP optimal_interval 后端，且 optimal_retention
        // 曲面被暴露（None→Some）。无 ssp_config 时 optimal_retention=None（MDM 路径，bit-exact）。
        let mut config = MemoryModelConfig::default();
        config.gsp_interval_cap_days = 40.0; // 确保 GSP head 激活 → scheduled_interval_days 为 Some
        let mk_item = || BenchmarkHistoryItem {
            t_history: vec![0, 3, 7, 15],
            r_history: vec![1, 1, 1, 1],
            t_history_csv: None,
            r_history_csv: None,
            next_t_days: Some(5.0),
            target_retentions: vec![0.85],
            interval_scale: 1.0,
        };

        let mdm = evaluate_batch(BenchmarkAdapterRequest {
            config: config.clone(),
            items: vec![mk_item()],
            ssp_config: None,
        })
        .expect("mdm response");
        assert!(mdm.items[0].optimal_retention.is_none());
        assert!(mdm.items[0].scheduled_interval_days.is_some());

        let ssp_cfg = SspConfig {
            max_iterations: 50,
            ..Default::default()
        };
        let ssp = evaluate_batch(BenchmarkAdapterRequest {
            config,
            items: vec![mk_item()],
            ssp_config: Some(ssp_cfg.clone()),
        })
        .expect("ssp response");
        let dr = ssp.items[0]
            .optimal_retention
            .expect("SSP 后端应暴露 DR 曲面");
        assert!(
            dr >= ssp_cfg.r_min - 1e-9 && dr <= ssp_cfg.r_max + 1e-9,
            "状态相关 DR {dr} 应 ∈ [{}, {}]",
            ssp_cfg.r_min,
            ssp_cfg.r_max
        );
        assert!(ssp.items[0].scheduled_interval_days.is_some());
    }
}
