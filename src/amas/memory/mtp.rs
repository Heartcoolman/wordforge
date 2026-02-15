//! B37: Morpheme Transfer Prediction (MTP)
//! Known morphemes boost learning efficiency for words sharing those morphemes.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::amas::config::MtpConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MtpState {
    /// Known morphemes and their familiarity scores
    pub known_morphemes: Vec<(String, f64)>,
}

/// Calculate the morpheme transfer bonus.
/// If the word shares known morphemes, learning is boosted.
pub fn morpheme_transfer_bonus(
    word_morphemes: &[String],
    known: &[(String, f64)],
    config: &MtpConfig,
) -> f64 {
    if word_morphemes.is_empty() || known.is_empty() {
        return 0.0;
    }

    let known_set: HashSet<&str> = known.iter().map(|(m, _)| m.as_str()).collect();
    let mut total_bonus = 0.0;
    let mut match_count = 0;

    for morpheme in word_morphemes {
        if !known_set.contains(morpheme.as_str()) {
            continue;
        }
        if let Some((_, familiarity)) = known.iter().find(|(m, _)| m == morpheme) {
            total_bonus += familiarity * config.morpheme_transfer_coeff;
            match_count += 1;
        }
    }

    if match_count > 0 {
        (total_bonus / match_count as f64).clamp(0.0, config.morpheme_bonus_cap)
    } else {
        0.0
    }
}

/// Update known morphemes after successfully learning a word
pub fn update_known_morphemes(
    state: &mut MtpState,
    word_morphemes: &[String],
    quality: f64,
    config: &MtpConfig,
) {
    for morpheme in word_morphemes {
        let found = state
            .known_morphemes
            .iter_mut()
            .find(|(m, _)| m == morpheme);
        match found {
            Some((_, familiarity)) => {
                *familiarity = (*familiarity * config.known_morpheme_decay
                    + quality * (1.0 - config.known_morpheme_decay))
                    .clamp(0.0, 1.0);
            }
            None => {
                state.known_morphemes.push((
                    morpheme.clone(),
                    quality * config.new_morpheme_initial_coeff,
                ));
            }
        }
    }

    if state.known_morphemes.len() > config.max_known_morphemes {
        state
            .known_morphemes
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        state.known_morphemes.truncate(config.max_known_morphemes);
    }
}
