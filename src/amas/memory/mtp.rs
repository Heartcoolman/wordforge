//! B37: Morpheme Transfer Prediction (MTP)
//! Known morphemes boost learning efficiency for words sharing those morphemes.

use serde::{Deserialize, Serialize};

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
) -> f64 {
    if word_morphemes.is_empty() || known.is_empty() {
        return 0.0;
    }

    let mut total_bonus = 0.0;
    let mut match_count = 0;

    for morpheme in word_morphemes {
        for (known_m, familiarity) in known {
            if morpheme == known_m {
                total_bonus += familiarity * 0.15;
                match_count += 1;
            }
        }
    }

    if match_count > 0 {
        (total_bonus / match_count as f64).clamp(0.0, 0.3)
    } else {
        0.0
    }
}

/// Update known morphemes after successfully learning a word
pub fn update_known_morphemes(
    state: &mut MtpState,
    word_morphemes: &[String],
    quality: f64,
) {
    for morpheme in word_morphemes {
        let found = state.known_morphemes.iter_mut().find(|(m, _)| m == morpheme);
        match found {
            Some((_, familiarity)) => {
                *familiarity = (*familiarity * 0.9 + quality * 0.1).clamp(0.0, 1.0);
            }
            None => {
                state.known_morphemes.push((morpheme.clone(), quality * 0.5));
            }
        }
    }

    if state.known_morphemes.len() > 500 {
        state.known_morphemes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        state.known_morphemes.truncate(500);
    }
}
