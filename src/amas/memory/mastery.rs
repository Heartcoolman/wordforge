use serde::{Deserialize, Serialize};

use crate::amas::config::MemoryModelConfig;
use crate::amas::types::*;

use super::mdm::MdmState;

const ALPHA_SCALE: f64 = 0.3;
const ALPHA_MIN: f64 = 0.1;
const ALPHA_MAX: f64 = 0.5;
const FORGETTING_THRESHOLD: f64 = 0.2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordMasteryState {
    pub word_id: String,
    pub mdm: MdmState,
    pub mastery_level: MasteryLevel,
    pub correct_streak: u32,
    pub total_attempts: u32,
    pub total_correct: u32,
}

impl WordMasteryState {
    pub fn new(word_id: &str) -> Self {
        Self {
            word_id: word_id.to_string(),
            mdm: MdmState::default(),
            mastery_level: MasteryLevel::New,
            correct_streak: 0,
            total_attempts: 0,
            total_correct: 0,
        }
    }
}

pub fn update_mastery(
    state: &mut WordMasteryState,
    is_correct: bool,
    quality: f64,
    interval_scale: f64,
    desired_retention: f64,
    config: &MemoryModelConfig,
) -> WordMasteryDecision {
    let alpha = (interval_scale * ALPHA_SCALE).clamp(ALPHA_MIN, ALPHA_MAX);
    super::mdm::update_strength(&mut state.mdm, quality, alpha, config);

    state.total_attempts += 1;
    if is_correct {
        state.total_correct += 1;
        state.correct_streak += 1;
    } else {
        state.correct_streak = 0;
    }

    state.mastery_level = determine_level(state, config);

    let now = chrono::Utc::now().timestamp_millis();
    let recall = super::mdm::recall_probability(&state.mdm, now, config);
    let interval =
        super::mdm::compute_interval(&state.mdm, desired_retention, interval_scale, config);

    WordMasteryDecision {
        word_id: state.word_id.clone(),
        memory_strength: state.mdm.memory_strength,
        recall_probability: recall,
        next_review_interval_secs: interval,
        mastery_level: state.mastery_level.clone(),
    }
}

fn determine_level(state: &WordMasteryState, config: &MemoryModelConfig) -> MasteryLevel {
    let accuracy = if state.total_attempts > 0 {
        state.total_correct as f64 / state.total_attempts as f64
    } else {
        0.0
    };
    let composite = super::mdm::composite_strength(&state.mdm, config);

    if state.total_attempts == 0 {
        MasteryLevel::New
    } else if composite > config.mastery_composite_threshold
        && accuracy > config.mastery_accuracy_threshold
        && state.correct_streak >= config.mastery_streak_threshold
    {
        MasteryLevel::Mastered
    } else {
        let now = chrono::Utc::now().timestamp_millis();
        let recall = super::mdm::recall_probability(&state.mdm, now, config);
        if recall < FORGETTING_THRESHOLD {
            MasteryLevel::Forgotten
        } else if composite > config.reviewing_threshold {
            MasteryLevel::Reviewing
        } else {
            MasteryLevel::Learning
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_up_after_correct_streak() {
        let config = MemoryModelConfig::default();
        let mut state = WordMasteryState::new("w1");
        for _ in 0..5 {
            let _ = update_mastery(&mut state, true, 0.95, 1.0, 0.9, &config);
        }
        assert!(matches!(
            state.mastery_level,
            MasteryLevel::Reviewing | MasteryLevel::Mastered
        ));
    }
}
