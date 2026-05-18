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
    let total_interference: f64 = state
        .confusion_pairs
        .iter()
        .filter(|(id, _)| id == word_id)
        .map(|(_, score)| score)
        .sum();

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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> IadConfig {
        IadConfig::default()
    }

    #[test]
    fn interference_penalty_empty_state_returns_zero() {
        let state = IadState::default();
        assert_eq!(interference_penalty("w", &state, &cfg()), 0.0);
    }

    #[test]
    fn interference_penalty_caps_to_config_value() {
        let cfg = IadConfig {
            interference_penalty_factor: 1.0,
            interference_penalty_cap: 0.3,
            ..Default::default()
        };
        let state = IadState {
            confusion_pairs: vec![("w".into(), 5.0), ("w".into(), 5.0)],
        };
        let p = interference_penalty("w", &state, &cfg);
        assert!((p - 0.3).abs() < 1e-9);
    }

    #[test]
    fn record_confusion_adds_bidirectional_pairs() {
        let mut state = IadState::default();
        record_confusion(&mut state, "a", "b", 0.0, &cfg());
        // 两个方向都加入
        let ids: Vec<&String> = state.confusion_pairs.iter().map(|(id, _)| id).collect();
        assert!(ids.contains(&&"a".to_string()));
        assert!(ids.contains(&&"b".to_string()));
    }

    #[test]
    fn record_confusion_decays_existing_then_increments() {
        let cfg = cfg();
        let mut state = IadState {
            confusion_pairs: vec![("a".into(), 0.5), ("b".into(), 0.5)],
        };
        record_confusion(&mut state, "a", "b", 0.5, &cfg);
        // 旧 0.5 * 0.5 = 0.25，然后被增量 +0.2 = 0.45（clamped 1.0）
        let a = state.confusion_pairs.iter().find(|(id, _)| id == "a").unwrap();
        let b = state.confusion_pairs.iter().find(|(id, _)| id == "b").unwrap();
        assert!((a.1 - 0.45).abs() < 1e-9);
        assert!((b.1 - 0.45).abs() < 1e-9);
    }

    #[test]
    fn record_confusion_clamps_at_one() {
        let mut cfg = cfg();
        cfg.confusion_update_increment = 5.0;
        let mut state = IadState {
            confusion_pairs: vec![("a".into(), 0.5)],
        };
        record_confusion(&mut state, "a", "b", 0.0, &cfg);
        let a = state.confusion_pairs.iter().find(|(id, _)| id == "a").unwrap();
        assert!((a.1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn record_confusion_truncates_to_max_pairs() {
        let mut cfg = cfg();
        cfg.max_confusion_pairs = 2;
        let mut state = IadState {
            confusion_pairs: vec![
                ("x".into(), 0.9),
                ("y".into(), 0.8),
                ("z".into(), 0.7),
            ],
        };
        record_confusion(&mut state, "a", "b", 0.0, &cfg);
        assert_eq!(state.confusion_pairs.len(), 2);
        // 最高 score 在前
        assert!(state.confusion_pairs[0].1 >= state.confusion_pairs[1].1);
    }

    #[test]
    fn interval_extension_factor_is_one_minus_scaled_penalty() {
        let cfg = IadConfig {
            interval_shortening_factor: 0.5,
            ..Default::default()
        };
        assert!((interval_extension_factor(0.0, &cfg) - 1.0).abs() < 1e-9);
        assert!((interval_extension_factor(1.0, &cfg) - 0.5).abs() < 1e-9);
    }
}
