use serde::{Deserialize, Serialize};

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

pub fn update_strength(state: &mut MdmState, quality: f64, alpha: f64) {
    let quality = quality.clamp(0.0, 1.0);

    state.short_term_strength += 0.50 * (quality - state.short_term_strength);
    state.medium_term_strength += 0.20 * (quality - state.medium_term_strength);
    state.long_term_strength += 0.05 * (quality - state.long_term_strength);

    state.short_term_strength = state.short_term_strength.clamp(0.0, 1.0);
    state.medium_term_strength = state.medium_term_strength.clamp(0.0, 1.0);
    state.long_term_strength = state.long_term_strength.clamp(0.0, 1.0);

    // B36: Update consolidation alongside memory_strength
    // Consolidation grows slowly through successful reviews
    let consolidation_rate = 0.03 * quality;
    state.consolidation = (state.consolidation + consolidation_rate).clamp(0.0, 1.0);

    let composite = composite_strength(state);
    // B36: Vocabulary-specific correction using consolidation
    let vocab_corrected = composite * (1.0 + state.consolidation * 0.2);
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
    if accuracy > 0.9 {
        retention += 0.02;
    }

    // High fatigue -> lower retention target (easier to reach)
    if fatigue > 0.6 {
        retention -= 0.05;
    }

    // Low motivation -> slightly lower target
    if motivation < -0.2 {
        retention -= 0.03;
    }

    retention.clamp(0.70, 0.95)
}

pub fn composite_strength(state: &MdmState) -> f64 {
    (0.20 * state.short_term_strength
        + 0.30 * state.medium_term_strength
        + 0.50 * state.long_term_strength)
        .clamp(0.0, 1.0)
}

pub fn recall_probability(state: &MdmState, now_ms: i64) -> f64 {
    let eps = 0.1;
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let delta_secs = ((now_ms - last) as f64 / 1000.0).max(0.0);
            let half_life_secs = (state.memory_strength.max(0.0) + eps) * 86400.0;
            (-delta_secs / half_life_secs).exp().clamp(0.0, 1.0)
        }
    }
}

pub fn compute_interval(state: &MdmState, target_recall: f64, interval_scale: f64) -> i64 {
    let eps = 0.1;
    let half_life = (state.memory_strength.max(0.0) + eps) * 86400.0;
    let interval = -half_life * target_recall.max(1e-6).ln();
    (interval * interval_scale.max(0.1)).min(365.0 * 86400.0) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_is_bounded_and_monotonic() {
        let mut state = MdmState::default();
        update_strength(&mut state, 0.8, 0.3);
        let t1 = state.last_review_at.unwrap() + 1000;
        let t2 = state.last_review_at.unwrap() + 5000;
        let p1 = recall_probability(&state, t1);
        let p2 = recall_probability(&state, t2);
        assert!((0.0..=1.0).contains(&p1));
        assert!((0.0..=1.0).contains(&p2));
        assert!(p2 <= p1);
    }

    #[test]
    fn composite_strength_moves_up_after_good_quality() {
        let mut state = MdmState::default();
        let before = composite_strength(&state);
        update_strength(&mut state, 0.9, 0.3);
        let after = composite_strength(&state);
        assert!(after >= before);
    }
}
