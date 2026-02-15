//! B41-B42: ELO rating system for user-word difficulty matching
//! and ZPD (Zone of Proximal Development) based word prioritization.

use serde::{Deserialize, Serialize};

use crate::amas::config::EloConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EloRating {
    pub rating: f64,
    pub games: u32,
}

impl Default for EloRating {
    fn default() -> Self {
        Self {
            rating: EloConfig::default().default_elo,
            games: 0,
        }
    }
}

/// Expected score for player A against player B
fn expected_score(rating_a: f64, rating_b: f64) -> f64 {
    1.0 / (1.0 + 10.0_f64.powf((rating_b - rating_a) / 400.0))
}

/// Update ELO ratings after an answer
/// Returns (new_user_elo, new_word_elo)
pub fn update_elo(
    user_elo: &mut EloRating,
    word_elo: &mut EloRating,
    is_correct: bool,
    config: &EloConfig,
) -> (f64, f64) {
    let expected_user = expected_score(user_elo.rating, word_elo.rating);
    let actual = if is_correct { 1.0 } else { 0.0 };

    // Adaptive K-factor: higher for new players
    let k_user = if user_elo.games < config.novice_game_threshold {
        config.k_factor * config.novice_k_multiplier
    } else {
        config.k_factor
    };
    let k_word = if word_elo.games < config.novice_game_threshold {
        config.k_factor * config.novice_k_multiplier * config.word_k_factor_ratio
    } else {
        config.k_factor * config.word_k_factor_ratio
    };

    user_elo.rating =
        (user_elo.rating + k_user * (actual - expected_user)).clamp(config.min_elo, config.max_elo);
    word_elo.rating =
        (word_elo.rating + k_word * (expected_user - actual)).clamp(config.min_elo, config.max_elo);

    user_elo.games += 1;
    word_elo.games += 1;

    (user_elo.rating, word_elo.rating)
}

/// B42: ZPD priority - rank words by proximity to user's ELO
/// Words in the "zone of proximal development" are most beneficial
pub fn zpd_priority(user_elo: f64, word_elo: f64, config: &EloConfig) -> f64 {
    let signed_distance = word_elo - user_elo - config.zpd_optimal_offset;
    (-signed_distance.powi(2) / (2.0 * config.zpd_gaussian_sigma.powi(2))).exp()
}

/// Sort word IDs by ZPD priority (best first)
pub fn rank_by_zpd(
    user_elo: f64,
    words: &[(String, f64)], // (word_id, word_elo)
    config: &EloConfig,
) -> Vec<(String, f64)> {
    let mut ranked: Vec<(String, f64)> = words
        .iter()
        .map(|(id, elo)| (id.clone(), zpd_priority(user_elo, *elo, config)))
        .collect();

    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elo_converges() {
        let config = EloConfig::default();
        let mut user = EloRating::default();
        let mut word = EloRating::default();

        // User always correct -> user rating goes up, word goes down
        for _ in 0..20 {
            update_elo(&mut user, &mut word, true, &config);
        }
        assert!(user.rating > config.default_elo);
        assert!(word.rating < config.default_elo);
    }

    #[test]
    fn zpd_priority_peaks_near_user() {
        let config = EloConfig::default();
        let user_elo = 1200.0;
        let p_close = zpd_priority(user_elo, 1300.0, &config);
        let p_far = zpd_priority(user_elo, 1800.0, &config);
        assert!(p_close > p_far);
    }
}
