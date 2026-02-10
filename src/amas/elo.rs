//! B41-B42: ELO rating system for user-word difficulty matching
//! and ZPD (Zone of Proximal Development) based word prioritization.

use serde::{Deserialize, Serialize};

/// K-factor for ELO updates
const K_FACTOR: f64 = 32.0;

/// Default ELO rating
const DEFAULT_ELO: f64 = 1200.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EloRating {
    pub rating: f64,
    pub games: u32,
}

impl Default for EloRating {
    fn default() -> Self {
        Self {
            rating: DEFAULT_ELO,
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
) -> (f64, f64) {
    let expected_user = expected_score(user_elo.rating, word_elo.rating);
    let actual = if is_correct { 1.0 } else { 0.0 };

    // Adaptive K-factor: higher for new players
    let k_user = if user_elo.games < 30 {
        K_FACTOR * 2.0
    } else {
        K_FACTOR
    };
    let k_word = if word_elo.games < 30 {
        K_FACTOR * 2.0
    } else {
        K_FACTOR
    };

    user_elo.rating += k_user * (actual - expected_user);
    word_elo.rating += k_word * (expected_user - actual);

    user_elo.games += 1;
    word_elo.games += 1;

    (user_elo.rating, word_elo.rating)
}

/// B42: ZPD priority - rank words by proximity to user's ELO
/// Words in the "zone of proximal development" are most beneficial
pub fn zpd_priority(user_elo: f64, word_elo: f64) -> f64 {
    let optimal_offset = 100.0;
    let signed_distance = word_elo - user_elo - optimal_offset;
    (-signed_distance.powi(2) / (2.0 * 150.0_f64.powi(2))).exp()
}

/// Sort word IDs by ZPD priority (best first)
pub fn rank_by_zpd(
    user_elo: f64,
    words: &[(String, f64)], // (word_id, word_elo)
) -> Vec<(String, f64)> {
    let mut ranked: Vec<(String, f64)> = words
        .iter()
        .map(|(id, elo)| (id.clone(), zpd_priority(user_elo, *elo)))
        .collect();

    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    });

    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elo_converges() {
        let mut user = EloRating::default();
        let mut word = EloRating::default();

        // User always correct -> user rating goes up, word goes down
        for _ in 0..20 {
            update_elo(&mut user, &mut word, true);
        }
        assert!(user.rating > DEFAULT_ELO);
        assert!(word.rating < DEFAULT_ELO);
    }

    #[test]
    fn zpd_priority_peaks_near_user() {
        let user_elo = 1200.0;
        let p_close = zpd_priority(user_elo, 1300.0);
        let p_far = zpd_priority(user_elo, 1800.0);
        assert!(p_close > p_far);
    }
}
