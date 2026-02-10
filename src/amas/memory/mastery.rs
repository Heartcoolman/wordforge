use serde::{Deserialize, Serialize};

use crate::amas::types::*;

use super::mdm::MdmState;

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
) -> WordMasteryDecision {
    let alpha = (interval_scale * 0.3).clamp(0.1, 0.5);
    super::mdm::update_strength(&mut state.mdm, quality, alpha);

    state.total_attempts += 1;
    if is_correct {
        state.total_correct += 1;
        state.correct_streak += 1;
    } else {
        state.correct_streak = 0;
    }

    state.mastery_level = determine_level(state);

    let now = chrono::Utc::now().timestamp_millis();
    let recall = super::mdm::recall_probability(&state.mdm, now);
    let interval = super::mdm::compute_interval(&state.mdm, 0.85, interval_scale);

    WordMasteryDecision {
        word_id: state.word_id.clone(),
        memory_strength: state.mdm.memory_strength,
        recall_probability: recall,
        next_review_interval_secs: interval,
        mastery_level: state.mastery_level.clone(),
    }
}

fn determine_level(state: &WordMasteryState) -> MasteryLevel {
    let accuracy = if state.total_attempts > 0 {
        state.total_correct as f64 / state.total_attempts as f64
    } else {
        0.0
    };
    let composite = super::mdm::composite_strength(&state.mdm);

    if state.total_attempts == 0 {
        MasteryLevel::New
    } else if composite > 0.8 && accuracy > 0.9 && state.correct_streak >= 3 {
        MasteryLevel::Mastered
    } else if composite > 0.4 {
        MasteryLevel::Reviewing
    } else {
        MasteryLevel::Learning
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_up_after_correct_streak() {
        let mut state = WordMasteryState::new("w1");
        for _ in 0..5 {
            let _ = update_mastery(&mut state, true, 0.95, 1.0);
        }
        assert!(matches!(
            state.mastery_level,
            MasteryLevel::Reviewing | MasteryLevel::Mastered
        ));
    }
}
