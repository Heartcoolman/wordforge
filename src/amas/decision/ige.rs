use serde::{Deserialize, Serialize};

use crate::amas::config::AMASConfig;
use crate::amas::types::*;

const UNEXPLORED_BIN_SCORE: f64 = 1e6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgeState {
    pub difficulty_bins: Vec<BinStats>,
    pub ratio_bins: Vec<BinStats>,
    pub total_explorations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinStats {
    pub range_start: f64,
    pub range_end: f64,
    pub count: u64,
    pub avg_reward: f64,
    pub variance: f64,
}

impl Default for IgeState {
    fn default() -> Self {
        Self {
            difficulty_bins: vec![
                BinStats::new(0.0, 0.2),
                BinStats::new(0.2, 0.4),
                BinStats::new(0.4, 0.6),
                BinStats::new(0.6, 0.8),
                BinStats::new(0.8, 1.0),
            ],
            ratio_bins: vec![
                BinStats::new(0.0, 0.25),
                BinStats::new(0.25, 0.5),
                BinStats::new(0.5, 0.75),
                BinStats::new(0.75, 1.0),
            ],
            total_explorations: 0,
        }
    }
}

impl BinStats {
    fn new(range_start: f64, range_end: f64) -> Self {
        Self {
            range_start,
            range_end,
            count: 0,
            avg_reward: 0.0,
            variance: 0.0,
        }
    }

    fn midpoint(&self) -> f64 {
        (self.range_start + self.range_end) / 2.0
    }
}

pub fn generate(
    _user_state: &UserState,
    _feature: &FeatureVector,
    ige_state: &IgeState,
    config: &AMASConfig,
) -> DecisionCandidate {
    let ige = &config.ige;
    let ucb_coeff = ige.ucb_confidence_coeff;

    let diff_total = ige_state
        .difficulty_bins
        .iter()
        .map(|b| b.count)
        .sum::<u64>()
        .max(1) as f64;
    let ratio_total = ige_state
        .ratio_bins
        .iter()
        .map(|b| b.count)
        .sum::<u64>()
        .max(1) as f64;

    let best_diff = ige_state
        .difficulty_bins
        .iter()
        .max_by(|a, b| {
            ucb(a, diff_total, ucb_coeff)
                .partial_cmp(&ucb(b, diff_total, ucb_coeff))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
        .unwrap_or_else(|| BinStats::new(0.4, 0.6));

    let best_ratio = ige_state
        .ratio_bins
        .iter()
        .max_by(|a, b| {
            ucb(a, ratio_total, ucb_coeff)
                .partial_cmp(&ucb(b, ratio_total, ucb_coeff))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
        .unwrap_or_else(|| BinStats::new(0.25, 0.5));

    DecisionCandidate {
        algorithm_id: AlgorithmId::Ige,
        strategy: StrategyParams {
            difficulty: best_diff.midpoint().clamp(0.0, 1.0),
            batch_size: ige.batch_size,
            new_ratio: best_ratio.midpoint().clamp(0.0, 1.0),
            interval_scale: ige.interval_scale,
            review_mode: false,
        },
        confidence: ige.default_confidence,
        explanation: "IGE exploration strategy".to_string(),
    }
}

fn ucb(bin: &BinStats, total: f64, ucb_coeff: f64) -> f64 {
    if bin.count == 0 {
        // 为未探索 bin 添加微小随机扰动，打破对称性
        return UNEXPLORED_BIN_SCORE + rand::random::<f64>() * 0.01;
    }
    let count = bin.count as f64;
    bin.avg_reward + (ucb_coeff * total.ln() / count).sqrt()
}

pub fn update(ige_state: &mut IgeState, strategy: &StrategyParams, reward: f64) {
    if let Some(bin) = find_bin_mut(&mut ige_state.difficulty_bins, strategy.difficulty) {
        update_bin(bin, reward);
    }
    if let Some(bin) = find_bin_mut(&mut ige_state.ratio_bins, strategy.new_ratio) {
        update_bin(bin, reward);
    }
    ige_state.total_explorations += 1;
}

fn find_bin_mut(bins: &mut [BinStats], value: f64) -> Option<&mut BinStats> {
    let clamped = value.clamp(0.0, 1.0);
    let len = bins.len();
    bins.iter_mut()
        .enumerate()
        .find(|(i, bin)| clamped >= bin.range_start && (clamped < bin.range_end || *i == len - 1))
        .map(|(_, bin)| bin)
}

fn update_bin(bin: &mut BinStats, reward: f64) {
    let old_avg = bin.avg_reward;
    let old_count = bin.count as f64;
    bin.count += 1;
    let n = bin.count as f64;
    bin.avg_reward += (reward - bin.avg_reward) / n;
    // Welford's online variance: reconstruct M2 from old count, then update
    let m2 = bin.variance * old_count;
    let new_m2 = m2 + (reward - old_avg) * (reward - bin.avg_reward);
    bin.variance = if n > 1.0 { new_m2 / (n - 1.0) } else { 0.0 };
}
