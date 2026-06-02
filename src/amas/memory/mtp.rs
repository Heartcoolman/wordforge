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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MtpConfig {
        MtpConfig::default()
    }

    #[test]
    fn bonus_zero_when_word_morphemes_or_known_empty() {
        assert_eq!(
            morpheme_transfer_bonus(&[], &[("a".into(), 0.5)], &cfg()),
            0.0
        );
        assert_eq!(
            morpheme_transfer_bonus(&["a".to_string()], &[], &cfg()),
            0.0
        );
    }

    #[test]
    fn bonus_zero_when_no_overlap() {
        let v = morpheme_transfer_bonus(&["x".to_string()], &[("a".into(), 1.0)], &cfg());
        assert_eq!(v, 0.0);
    }

    #[test]
    fn bonus_averages_familiarity_and_clamps_to_cap() {
        let cfg = MtpConfig {
            morpheme_transfer_coeff: 1.0,
            morpheme_bonus_cap: 0.3,
            ..Default::default()
        };
        // 两个匹配，平均 0.5
        let v = morpheme_transfer_bonus(
            &["a".to_string(), "b".to_string()],
            &[("a".into(), 0.4), ("b".into(), 0.6)],
            &cfg,
        );
        // (0.4*1.0 + 0.6*1.0)/2 = 0.5，被 cap 到 0.3
        assert!((v - 0.3).abs() < 1e-9);
    }

    #[test]
    fn update_known_morphemes_creates_and_decays() {
        let cfg = MtpConfig {
            new_morpheme_initial_coeff: 0.5,
            known_morpheme_decay: 0.5,
            ..Default::default()
        };
        let mut state = MtpState::default();
        update_known_morphemes(&mut state, &["a".into()], 1.0, &cfg);
        // 新词：quality * new_morpheme_initial_coeff = 0.5
        assert!((state.known_morphemes[0].1 - 0.5).abs() < 1e-9);

        // 第二次：0.5 * 0.5 + 1.0 * 0.5 = 0.75
        update_known_morphemes(&mut state, &["a".into()], 1.0, &cfg);
        assert!((state.known_morphemes[0].1 - 0.75).abs() < 1e-9);
    }

    #[test]
    fn update_known_morphemes_truncates_at_max() {
        let cfg = MtpConfig {
            max_known_morphemes: 2,
            ..Default::default()
        };
        let mut state = MtpState {
            known_morphemes: vec![("x".into(), 0.9), ("y".into(), 0.5), ("z".into(), 0.1)],
        };
        update_known_morphemes(&mut state, &["new".into()], 1.0, &cfg);
        assert_eq!(state.known_morphemes.len(), 2);
        // 高 familiarity 优先保留
        assert!(state.known_morphemes[0].1 >= state.known_morphemes[1].1);
    }

    #[test]
    fn update_known_morphemes_clamps_to_one() {
        let cfg = MtpConfig {
            known_morpheme_decay: 0.0,
            ..Default::default()
        };
        let mut state = MtpState {
            known_morphemes: vec![("a".into(), 0.5)],
        };
        update_known_morphemes(&mut state, &["a".into()], 2.0, &cfg);
        // 0.5*0 + 2.0*1 = 2.0 -> clamp 1.0
        assert!((state.known_morphemes[0].1 - 1.0).abs() < 1e-9);
    }
}
