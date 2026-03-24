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
const MAX_INTERVAL_DAYS: f64 = 90.0;
const MIN_INTERVAL_SECS: i64 = 60;

/// AMAS v2: DSR (Difficulty-Stability-Retrievability) architecture
/// Uses FSRS-5 formulas for state transitions with AMAS-unique scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdmState {
    /// Stability: interval in days at which R = 90%.
    #[serde(default = "default_stability")]
    pub stability: f64,
    /// Difficulty: D ∈ [1, 10]. Higher = harder material.
    #[serde(default = "default_difficulty")]
    pub difficulty: f64,
    /// Backward-compatible alias: equals stability for downstream code.
    #[serde(default)]
    pub memory_strength: f64,
    pub last_review_at: Option<i64>,
    pub review_count: u32,
    // Legacy fields – kept for serde backward compat
    #[serde(default)]
    pub short_term_strength: f64,
    #[serde(default)]
    pub medium_term_strength: f64,
    #[serde(default)]
    pub long_term_strength: f64,
    #[serde(default)]
    pub consolidation: f64,
}

fn default_stability() -> f64 { 0.4 }
fn default_difficulty() -> f64 { 5.0 }

impl Default for MdmState {
    fn default() -> Self {
        Self {
            stability: 0.4,
            difficulty: 5.0,
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

/// Main DSR update function.
/// Uses FSRS-5 formulas for stability transitions.
///
/// quality: 0.0-1.0, mapped to FSRS grade:
///   quality <= 0.15 → Again(1), <= 0.5 → Hard(2), <= 0.85 → Good(3), else Easy(4)
pub fn update_strength(
    state: &mut MdmState,
    quality: f64,
    _alpha: f64, // kept for API compat
    now_ms: i64,
    config: &MemoryModelConfig,
) {
    let quality = quality.clamp(0.0, 1.0);
    // Map quality to FSRS grade
    let grade: u32 = if quality <= 0.15 { 1 }
        else if quality <= 0.5 { 2 }
        else if quality <= 0.85 { 3 }
        else { 4 };

    if state.review_count == 0 {
        // First review: initial stability from w0-w3
        let g = (grade as usize).clamp(1, 4) - 1;
        state.stability = config.w[g];
        // Initial difficulty: D0(G) = w4 - e^{w5*(G-1)} + 1
        state.difficulty = (config.w[4] - (config.w[5] * (grade as f64 - 1.0)).exp() + 1.0).clamp(1.0, 10.0);
    } else {
        // Compute current R
        let r = recall_probability(state, now_ms, config);

        // Update difficulty: D' = D - w6*(G-3), then mean reversion
        let delta_d = -config.w[6] * (grade as f64 - 3.0);
        let d_prime = state.difficulty + delta_d * (10.0 - state.difficulty) / 9.0;
        let d0_4 = (config.w[4] - (config.w[5] * 3.0).exp() + 1.0).clamp(1.0, 10.0);
        state.difficulty = (config.w[7] * d0_4 + (1.0 - config.w[7]) * d_prime).clamp(1.0, 10.0);

        if grade >= 2 {
            // Successful recall: S'_r = S * (e^w8 * (11-D) * S^{-w9} * (e^{w10*(1-R)} - 1) * bonus + 1)
            let bonus = if grade == 2 { config.w[15] }      // Hard
                else if grade == 4 { config.w[16] }           // Easy
                else { 1.0 };                                  // Good
            let s_inc = (config.w[8].exp()
                * (11.0 - state.difficulty)
                * state.stability.max(0.01).powf(-config.w[9])
                * ((config.w[10] * (1.0 - r)).exp() - 1.0)
                * bonus)
                .max(0.0);
            state.stability = (state.stability * (s_inc + 1.0)).max(0.01);
        } else {
            // Forgetting (Again): S'_f = w11 * D^{-w12} * ((S+1)^w13 - 1) * e^{w14*(1-R)}
            state.stability = (config.w[11]
                * state.difficulty.powf(-config.w[12])
                * ((state.stability + 1.0).powf(config.w[13]) - 1.0)
                * (config.w[14] * (1.0 - r)).exp())
                .clamp(0.01, state.stability);
        }
    }

    // Sync backward-compatible fields
    state.memory_strength = state.stability;
    let composite_val = (state.stability / 30.0).clamp(0.0, 1.0);
    state.short_term_strength = composite_val;
    state.medium_term_strength = composite_val;
    state.long_term_strength = composite_val;
    state.consolidation = (1.5 - state.difficulty / 10.0).clamp(0.0, 1.0);

    state.review_count += 1;
    state.last_review_at = Some(now_ms);
}

/// B40: Compute adaptive desired_retention based on various factors
pub fn adaptive_desired_retention(
    base_retention: f64,
    accuracy: f64,
    fatigue: f64,
    motivation: f64,
) -> f64 {
    let mut retention = base_retention;

    let sigmoid = |x: f64| 1.0 / (1.0 + (-x).exp());

    retention +=
        HIGH_ACCURACY_RETENTION_BOOST * sigmoid((accuracy - HIGH_ACCURACY_THRESHOLD) * 10.0);
    retention -= HIGH_FATIGUE_RETENTION_DROP * sigmoid((fatigue - HIGH_FATIGUE_THRESHOLD) * 10.0);
    retention -=
        LOW_MOTIVATION_RETENTION_DROP * sigmoid((LOW_MOTIVATION_THRESHOLD - motivation) * 10.0);

    retention.clamp(RETENTION_MIN, RETENTION_MAX)
}

/// Backward-compatible composite strength (normalized to [0,1])
pub fn composite_strength(state: &MdmState, _config: &MemoryModelConfig) -> f64 {
    (state.stability / 30.0).clamp(0.0, 1.0)
}

/// FSRS power-law forgetting curve with asymptote:
/// R(t,S) = floor + (1-floor) * (1 + factor * t/S)^decay
pub fn recall_probability(state: &MdmState, now_ms: i64, config: &MemoryModelConfig) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = ((now_ms - last) as f64 / 86_400_000.0).max(0.0);
            let s = state.stability.max(0.01);
            let power_law = (1.0 + config.forgetting_curve_factor * elapsed_days / s)
                .powf(config.forgetting_curve_decay);
            let floor = config.forgetting_curve_floor;
            (floor + (1.0 - floor) * power_law).clamp(0.0, 1.0)
        }
    }
}

/// Interval: solve R = floor + (1-floor) * (1 + factor*t/S)^decay for t
pub fn compute_interval(
    state: &MdmState,
    target_recall: f64,
    interval_scale: f64,
    config: &MemoryModelConfig,
) -> i64 {
    let s = state.stability.max(0.01);
    let floor = config.forgetting_curve_floor;
    let adjusted_target = ((target_recall - floor) / (1.0 - floor).max(1e-9)).max(1e-6).min(1.0);
    let interval_days = s / config.forgetting_curve_factor
        * (adjusted_target.powf(1.0 / config.forgetting_curve_decay) - 1.0);
    let interval_secs = interval_days * 86400.0;
    ((interval_secs * interval_scale.max(0.1)).min(MAX_INTERVAL_DAYS * 86400.0) as i64)
        .max(MIN_INTERVAL_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_is_bounded_and_monotonic() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let now_ms = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.8, 0.3, now_ms, &config);
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
        let now_ms = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.9, 0.3, now_ms, &config);
        let after = composite_strength(&state, &config);
        assert!(after >= before);
    }

    #[test]
    fn stability_grows_after_successful_recall() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let now = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.9, 0.3, now, &config);
        let s1 = state.stability;
        let later = now + 86_400_000;
        update_strength(&mut state, 0.9, 0.3, later, &config);
        let s2 = state.stability;
        assert!(s2 > s1, "Stability should grow after successful recall: {} > {}", s2, s1);
    }

    #[test]
    fn stability_decreases_after_forgetting() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let now = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.9, 0.3, now, &config);
        let s1 = state.stability;
        let later = now + 86_400_000;
        update_strength(&mut state, 0.0, 0.3, later, &config);
        let s2 = state.stability;
        assert!(s2 < s1, "Stability should decrease after forgetting: {} < {}", s2, s1);
    }
}
