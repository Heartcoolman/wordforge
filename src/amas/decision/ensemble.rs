// 集成模块硬编码为 3 个算法（Heuristic / IGE / SWD），因为：
// 1. 权重归一化、TrustScores 和 get_weights 均假定恰好 3 个参与者
// 2. min_weight 约束（3 * min_weight <= 1.0）同样依赖此数量
// 若需新增算法，需同步修改 TrustScores、get_weights、config 验证及 update_trust。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::amas::config::EnsembleConfig;
use crate::amas::types::{AlgorithmId, DecisionCandidate, StrategyParams};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustScores {
    pub heuristic: f64,
    pub ige: f64,
    pub swd: f64,
}

impl Default for TrustScores {
    fn default() -> Self {
        Self {
            heuristic: 0.5,
            ige: 0.5,
            swd: 0.5,
        }
    }
}

pub fn get_weights(
    total_samples: u64,
    trust_scores: &TrustScores,
    config: &EnsembleConfig,
) -> HashMap<AlgorithmId, f64> {
    let blend = if total_samples < config.warmup_samples {
        0.0
    } else {
        let raw = (total_samples - config.warmup_samples) as f64 / config.blend_scale;
        raw.min(config.blend_max)
    };

    let w_h = ((1.0 - blend) * config.base_weight_heuristic + blend * trust_scores.heuristic)
        .max(config.min_weight);
    let w_i =
        ((1.0 - blend) * config.base_weight_ige + blend * trust_scores.ige).max(config.min_weight);
    let w_s =
        ((1.0 - blend) * config.base_weight_swd + blend * trust_scores.swd).max(config.min_weight);

    let total = w_h + w_i + w_s;

    let mut weights = HashMap::new();
    weights.insert(AlgorithmId::Heuristic, w_h / total);
    weights.insert(AlgorithmId::Ige, w_i / total);
    weights.insert(AlgorithmId::Swd, w_s / total);
    weights
}

pub fn get_weights_for_candidates(
    candidates: &[DecisionCandidate],
    total_samples: u64,
    trust_scores: &TrustScores,
    config: &EnsembleConfig,
) -> HashMap<AlgorithmId, f64> {
    let all_weights = get_weights(total_samples, trust_scores, config);

    // Filter to only algorithms that produced candidates
    let candidate_ids: std::collections::HashSet<AlgorithmId> =
        candidates.iter().map(|c| c.algorithm_id).collect();
    let mut filtered: HashMap<AlgorithmId, f64> = all_weights
        .into_iter()
        .filter(|(id, _)| candidate_ids.contains(id))
        .collect();

    // Re-normalize
    let total: f64 = filtered.values().sum();
    if total > 0.0 {
        for v in filtered.values_mut() {
            *v /= total;
        }
    }
    filtered
}

pub fn merge(
    candidates: &[DecisionCandidate],
    weights: &HashMap<AlgorithmId, f64>,
) -> StrategyParams {
    let mut difficulty = 0.0;
    let mut batch_size_f = 0.0;
    let mut new_ratio = 0.0;
    let mut interval_scale = 0.0;
    let mut review_votes_for = 0.0;
    let mut review_votes_against = 0.0;

    for c in candidates {
        let w = match weights.get(&c.algorithm_id) {
            Some(&w) => w,
            None => {
                tracing::warn!(algorithm = ?c.algorithm_id, "Missing weight in ensemble merge, defaulting to 0");
                0.0
            }
        };
        difficulty += w * c.strategy.difficulty;
        batch_size_f += w * c.strategy.batch_size as f64;
        new_ratio += w * c.strategy.new_ratio;
        interval_scale += w * c.strategy.interval_scale;
        if c.strategy.review_mode {
            review_votes_for += w;
        } else {
            review_votes_against += w;
        }
    }

    StrategyParams {
        difficulty: difficulty.clamp(0.0, 1.0),
        batch_size: (batch_size_f.round() as u32).max(1),
        new_ratio: new_ratio.clamp(0.0, 1.0),
        interval_scale: interval_scale.max(0.1),
        review_mode: review_votes_for > review_votes_against,
    }
}

pub fn update_trust(
    trust_scores: &mut TrustScores,
    algorithm_id: AlgorithmId,
    reward: f64,
    learning_rate: f64,
) {
    let score = match algorithm_id {
        AlgorithmId::Heuristic => &mut trust_scores.heuristic,
        AlgorithmId::Ige => &mut trust_scores.ige,
        AlgorithmId::Swd => &mut trust_scores.swd,
        _ => return,
    };

    // Normalize reward from [-1, 1] to [0, 1] so negative feedback reduces trust
    let normalized = (reward.clamp(-1.0, 1.0) + 1.0) / 2.0;
    *score = *score * (1.0 - learning_rate) + normalized * learning_rate;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_sum_to_one() {
        let cfg = EnsembleConfig::default();
        let scores = TrustScores::default();
        let w = get_weights(10, &scores, &cfg);
        let sum: f64 = w.values().sum();
        assert!((sum - 1.0).abs() < 1e-9);
    }
}
