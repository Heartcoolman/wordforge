//! B38: Interference Aware Decay (IAD)
//! Confusion pair detection reduces retrievability and extends intervals.

use serde::{Deserialize, Serialize};

use crate::amas::config::IadConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IadState {
    /// Words that cause confusion (word_id, confusion_score)
    pub confusion_pairs: Vec<(String, f64)>,
}

/// Calculate interference penalty for a word based on confusion pairs.
/// Higher confusion scores mean more interference -> lower retrievability.
pub fn interference_penalty(word_id: &str, state: &IadState, config: &IadConfig) -> f64 {
    let mut total_interference = 0.0;

    for (confused_id, score) in &state.confusion_pairs {
        if confused_id == word_id {
            total_interference += score;
        }
    }

    (total_interference * config.interference_penalty_factor)
        .clamp(0.0, config.interference_penalty_cap)
}

/// Update confusion pairs when a word is confused with another.
/// Records bidirectional relationships: both (word_id, confused_with) and (confused_with, word_id).
pub fn record_confusion(
    state: &mut IadState,
    word_id: &str,
    confused_with: &str,
    decay_rate: f64,
    config: &IadConfig,
) {
    // Decay existing confusion scores
    for (_, score) in state.confusion_pairs.iter_mut() {
        *score *= 1.0 - decay_rate;
    }

    // Add or update bidirectional confusion pairs
    for target in &[confused_with, word_id] {
        let found = state
            .confusion_pairs
            .iter_mut()
            .find(|(id, _)| id == *target);

        match found {
            Some((_, score)) => {
                *score = (*score + config.confusion_update_increment).clamp(0.0, 1.0);
            }
            None => {
                state
                    .confusion_pairs
                    .push((target.to_string(), config.new_confusion_initial_score));
            }
        }
    }

    // Keep only top confusion pairs
    state
        .confusion_pairs
        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    state.confusion_pairs.truncate(config.max_confusion_pairs);
}

/// Compute interval extension factor based on interference
pub fn interval_extension_factor(penalty: f64, config: &IadConfig) -> f64 {
    // Higher interference -> shorter intervals (more review needed)
    1.0 - penalty * config.interval_shortening_factor
}
