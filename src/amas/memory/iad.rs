//! B38: Interference Aware Decay (IAD)
//! Confusion pair detection reduces retrievability and extends intervals.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IadState {
    /// Words that cause confusion (word_id, confusion_score)
    pub confusion_pairs: Vec<(String, f64)>,
}

/// Calculate interference penalty for a word based on confusion pairs.
/// Higher confusion scores mean more interference -> lower retrievability.
pub fn interference_penalty(word_id: &str, state: &IadState) -> f64 {
    let mut total_interference = 0.0;

    for (confused_id, score) in &state.confusion_pairs {
        if confused_id == word_id {
            total_interference += score;
        }
    }

    // Normalize penalty to [0, 0.3] range
    (total_interference * 0.1).clamp(0.0, 0.3)
}

/// Update confusion pairs when a word is confused with another
pub fn record_confusion(
    state: &mut IadState,
    word_id: &str,
    confused_with: &str,
    decay_rate: f64,
) {
    // Decay existing confusion scores
    for (_, score) in state.confusion_pairs.iter_mut() {
        *score *= 1.0 - decay_rate;
    }

    // Add or update confusion pair
    let found = state
        .confusion_pairs
        .iter_mut()
        .find(|(id, _)| id == confused_with);

    match found {
        Some((_, score)) => {
            *score = (*score + 0.2).clamp(0.0, 1.0);
        }
        None => {
            state
                .confusion_pairs
                .push((confused_with.to_string(), 0.2));
        }
    }

    // Keep only top confusion pairs
    state.confusion_pairs.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    });
    state.confusion_pairs.truncate(20);

    let _ = word_id; // used for context
}

/// Compute interval extension factor based on interference
pub fn interval_extension_factor(penalty: f64) -> f64 {
    // Higher interference -> shorter intervals (more review needed)
    1.0 - penalty * 0.5
}
