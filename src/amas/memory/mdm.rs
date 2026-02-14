use serde::{Deserialize, Serialize};

use crate::amas::config::MemoryModelConfig;

const HIGH_ACCURACY_THRESHOLD: f64 = 0.9;
const HIGH_ACCURACY_RETENTION_BOOST: f64 = 0.02;
const HIGH_FATIGUE_THRESHOLD: f64 = 0.6;
const HIGH_FATIGUE_RETENTION_DROP: f64 = 0.05;
const LOW_MOTIVATION_THRESHOLD: f64 = -0.2;
const LOW_MOTIVATION_RETENTION_DROP: f64 = 0.03;
const RETENTION_MIN: f64 = 0.70;
const RETENTION_MAX: f64 = 0.95;
const MAX_INTERVAL_DAYS: f64 = 365.0;
const MIN_INTERVAL_SECS: i64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdmState {
    pub memory_strength: f64,
    #[serde(default)]
    pub short_term_strength: f64,
    #[serde(default)]
    pub medium_term_strength: f64,
    #[serde(default)]
    pub long_term_strength: f64,
    pub last_review_at: Option<i64>,
    pub review_count: u32,
    // B36: Dual-dimension with consolidation
    #[serde(default)]
    pub consolidation: f64,
}

impl Default for MdmState {
    fn default() -> Self {
        Self {
            memory_strength: 0.0,
            short_term_strength: 0.0,
            medium_term_strength: 0.0,
            long_term_strength: 0.0,
            last_review_at: None,
            review_count: 0,
            consolidation: 0.0,
        }
    }
}

pub fn update_strength(state: &mut MdmState, quality: f64, alpha: f64, config: &MemoryModelConfig) {
    let quality = quality.clamp(0.0, 1.0);

    state.short_term_strength +=
        config.short_term_learning_rate * (quality - state.short_term_strength);
    state.medium_term_strength +=
        config.medium_term_learning_rate * (quality - state.medium_term_strength);
    state.long_term_strength +=
        config.long_term_learning_rate * (quality - state.long_term_strength);

    state.short_term_strength = state.short_term_strength.clamp(0.0, 1.0);
    state.medium_term_strength = state.medium_term_strength.clamp(0.0, 1.0);
    state.long_term_strength = state.long_term_strength.clamp(0.0, 1.0);

    // B36: Update consolidation alongside memory_strength
    // Consolidation grows slowly through successful reviews
    let consolidation_rate = config.consolidation_rate_scale * quality;
    state.consolidation = (state.consolidation + consolidation_rate).clamp(0.0, 1.0);

    let composite = composite_strength(state, config);
    // B36: Vocabulary-specific correction using consolidation
    let vocab_corrected = composite * (1.0 + state.consolidation * config.consolidation_bonus);
    state.memory_strength += alpha.clamp(0.0, 1.0) * (vocab_corrected - state.memory_strength);
    state.memory_strength = state.memory_strength.clamp(0.0, 1.0);

    state.review_count += 1;
    state.last_review_at = Some(chrono::Utc::now().timestamp_millis());
}

/// B40: Compute adaptive desired_retention based on various factors
pub fn adaptive_desired_retention(
    base_retention: f64,
    accuracy: f64,
    fatigue: f64,
    motivation: f64,
) -> f64 {
    let mut retention = base_retention;

    // High accuracy -> can push for higher retention
    if accuracy > HIGH_ACCURACY_THRESHOLD {
        retention += HIGH_ACCURACY_RETENTION_BOOST;
    }

    // High fatigue -> lower retention target (easier to reach)
    if fatigue > HIGH_FATIGUE_THRESHOLD {
        retention -= HIGH_FATIGUE_RETENTION_DROP;
    }

    // Low motivation -> slightly lower target
    if motivation < LOW_MOTIVATION_THRESHOLD {
        retention -= LOW_MOTIVATION_RETENTION_DROP;
    }

    retention.clamp(RETENTION_MIN, RETENTION_MAX)
}

pub fn composite_strength(state: &MdmState, config: &MemoryModelConfig) -> f64 {
    (config.composite_weight_short * state.short_term_strength
        + config.composite_weight_medium * state.medium_term_strength
        + config.composite_weight_long * state.long_term_strength)
        .clamp(0.0, 1.0)
}

pub fn recall_probability(state: &MdmState, now_ms: i64, config: &MemoryModelConfig) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let delta_secs = ((now_ms - last) as f64 / 1000.0).max(0.0);
            let time_constant_secs = (state.memory_strength.max(0.0) + config.half_life_base_epsilon)
                * config.half_life_time_unit_secs;
            (-delta_secs / time_constant_secs).exp().clamp(0.0, 1.0)
        }
    }
}

pub fn compute_interval(
    state: &MdmState,
    target_recall: f64,
    interval_scale: f64,
    config: &MemoryModelConfig,
) -> i64 {
    let time_constant = (state.memory_strength.max(0.0) + config.half_life_base_epsilon)
        * config.half_life_time_unit_secs;
    let interval = -time_constant * target_recall.max(1e-6).ln();
    ((interval * interval_scale.max(0.1)).min(MAX_INTERVAL_DAYS * 86400.0) as i64).max(MIN_INTERVAL_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_is_bounded_and_monotonic() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        update_strength(&mut state, 0.8, 0.3, &config);
        let t1 = state.last_review_at.unwrap() + 1000;
        let t2 = state.last_review_at.unwrap() + 5000;
        let p1 = recall_probability(&state, t1, &config);
        let p2 = recall_probability(&state, t2, &config);
        assert!((0.0..=1.0).contains(&p1));
        assert!((0.0..=1.0).contains(&p2));
        assert!(p2 <= p1);
    }

    #[test]
    fn composite_strength_moves_up_after_good_quality() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let before = composite_strength(&state, &config);
        update_strength(&mut state, 0.9, 0.3, &config);
        let after = composite_strength(&state, &config);
        assert!(after >= before);
    }
}
