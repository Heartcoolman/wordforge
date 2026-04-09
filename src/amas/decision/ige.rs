use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::amas::config::{AMASConfig, IgeConfig};
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

/// Bell-CDF bin boundaries: denser in the middle [0.3, 0.7].
/// Inverse CDF of Beta(2,2): solves 3x²-2x³ = p via Newton's method.
fn logistic_boundaries(count: usize) -> Vec<f64> {
    if count < 2 {
        return vec![0.0, 1.0];
    }
    let mut boundaries = Vec::with_capacity(count + 1);
    for i in 0..=count {
        let p = i as f64 / count as f64;
        boundaries.push(beta22_inv_cdf(p));
    }
    boundaries[0] = 0.0;
    boundaries[count] = 1.0;
    boundaries
}

fn beta22_inv_cdf(p: f64) -> f64 {
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    let mut x = p;
    for _ in 0..12 {
        let f = 3.0 * x * x - 2.0 * x * x * x - p;
        let fp = 6.0 * x * (1.0 - x);
        if fp.abs() < 1e-15 {
            break;
        }
        x -= f / fp;
    }
    x.clamp(0.0, 1.0)
}

fn generate_log_bins(count: usize) -> Vec<BinStats> {
    let bounds = logistic_boundaries(count);
    bounds
        .windows(2)
        .map(|w| BinStats::new(w[0], w[1]))
        .collect()
}

fn apply_pretrained_rewards(mut bins: Vec<BinStats>, rewards: &[f64]) -> Vec<BinStats> {
    for (bin, reward) in bins.iter_mut().zip(rewards.iter()) {
        bin.avg_reward = *reward;
        bin.count = 1;
    }
    bins
}

impl IgeState {
    pub fn new(config: &IgeConfig) -> Self {
        let mut diff_bins = generate_log_bins(config.difficulty_bin_count);
        let mut ratio_bins = generate_log_bins(config.ratio_bin_count);

        if let Some(ref rewards) = config.pretrained_difficulty_rewards {
            diff_bins = apply_pretrained_rewards(diff_bins, rewards);
        }
        if let Some(ref rewards) = config.pretrained_ratio_rewards {
            ratio_bins = apply_pretrained_rewards(ratio_bins, rewards);
        }

        Self {
            difficulty_bins: diff_bins,
            ratio_bins,
            total_explorations: 0,
        }
    }
}

impl Default for IgeState {
    fn default() -> Self {
        Self::new(&IgeConfig::default())
    }
}

pub fn generate(
    user_state: &UserState,
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
        .unwrap_or_else(|| {
            let mid = ige_state
                .difficulty_bins
                .get(ige_state.difficulty_bins.len() / 2)
                .cloned()
                .unwrap_or_else(|| BinStats::new(0.4, 0.6));
            mid
        });

    let best_ratio = ige_state
        .ratio_bins
        .iter()
        .max_by(|a, b| {
            ucb(a, ratio_total, ucb_coeff)
                .partial_cmp(&ucb(b, ratio_total, ucb_coeff))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
        .unwrap_or_else(|| {
            let mid = ige_state
                .ratio_bins
                .get(ige_state.ratio_bins.len() / 2)
                .cloned()
                .unwrap_or_else(|| BinStats::new(0.25, 0.5));
            mid
        });

    let mut difficulty = best_diff.midpoint().clamp(0.0, 1.0);
    if user_state.fatigue > 0.7 {
        difficulty = difficulty.min(0.5);
    }

    DecisionCandidate {
        algorithm_id: AlgorithmId::Ige,
        strategy: StrategyParams {
            difficulty,
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
        let mut hasher = std::hash::DefaultHasher::new();
        bin.range_start.to_bits().hash(&mut hasher);
        bin.range_end.to_bits().hash(&mut hasher);
        let h = hasher.finish();
        let deterministic_noise = (h % 10000) as f64 / 1_000_000.0;
        return UNEXPLORED_BIN_SCORE + deterministic_noise;
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
    let m2 = bin.variance * (old_count - 1.0).max(0.0);
    let new_m2 = m2 + (reward - old_avg) * (reward - bin.avg_reward);
    bin.variance = if n > 1.0 { new_m2 / (n - 1.0) } else { 0.0 };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_bins_cover_full_range() {
        let bins = generate_log_bins(20);
        assert!((bins[0].range_start - 0.0).abs() < 1e-9);
        assert!((bins.last().unwrap().range_end - 1.0).abs() < 1e-9);
        // Verify contiguous
        for w in bins.windows(2) {
            assert!((w[0].range_end - w[1].range_start).abs() < 1e-9);
        }
    }

    #[test]
    fn log_bins_denser_in_middle() {
        let bins = generate_log_bins(20);
        let first_span = bins[4].range_end - bins[0].range_start;
        let last_span = bins[19].range_end - bins[15].range_start;
        assert!((first_span - last_span).abs() < 0.05);
        let mid_avg = (bins[11].range_end - bins[8].range_start) / 3.0;
        let edge_avg = first_span / 4.0;
        assert!(
            mid_avg < edge_avg,
            "middle bins should be narrower: mid_avg={mid_avg} < edge_avg={edge_avg}"
        );
    }

    #[test]
    fn pretrained_rewards_applied() {
        let rewards = vec![0.5, 0.6, 0.7];
        let bins = generate_log_bins(3);
        let applied = apply_pretrained_rewards(bins, &rewards);
        for (bin, expected) in applied.iter().zip(rewards.iter()) {
            assert!((bin.avg_reward - expected).abs() < 1e-9);
            assert_eq!(bin.count, 1);
        }
    }

    #[test]
    fn ige_state_new_uses_config() {
        let config = IgeConfig {
            difficulty_bin_count: 10,
            ratio_bin_count: 8,
            ..Default::default()
        };
        let state = IgeState::new(&config);
        assert_eq!(state.difficulty_bins.len(), 10);
        assert_eq!(state.ratio_bins.len(), 8);
    }
}
