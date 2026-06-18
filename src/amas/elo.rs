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

/// T1.2 动态 K 乘子：`trend` 是该评分带符号残差的 EWMA（∈[-1,1]）。|trend| 大 = 近期残差同向
/// （能力/难度漂移）→ 增 K 追漂移；trend≈0（震荡/稳态）→ 乘子降到 `1 - k_trend_damp` 降噪。
/// 结果钳到 [k_min_factor, k_max_factor]。
fn dynamic_k_mult(trend: f64, config: &EloConfig) -> f64 {
    let base = (1.0 - config.k_trend_damp) + config.k_trend_gain * trend.abs();
    base.clamp(config.k_min_factor, config.k_max_factor)
}

/// T1.2 带趋势的 ELO 更新（动态 K）。`user_trend`/`word_trend` 是各自残差的持久化 EWMA，
/// 由本函数原地更新。`config.k_dynamic_enabled=false` 时与 [`update_elo`] **bit-exact 等价**
/// 且不触碰 trend（固定 K 路径）。novice 期把动态乘子下界钳为 1.0（保留新手快启动，只增不减）。
#[allow(clippy::too_many_arguments)]
pub fn update_elo_with_trend(
    user_elo: &mut EloRating,
    word_elo: &mut EloRating,
    user_trend: &mut f64,
    word_trend: &mut f64,
    is_correct: bool,
    config: &EloConfig,
) -> (f64, f64) {
    if !config.k_dynamic_enabled {
        return update_elo(user_elo, word_elo, is_correct, config);
    }

    let expected_user = expected_score(user_elo.rating, word_elo.rating);
    let actual = if is_correct { 1.0 } else { 0.0 };
    let user_residual = actual - expected_user; // 用户带符号残差
    let word_residual = expected_user - actual; // 词的残差（与用户镜像）

    // base K（含 novice 倍率），与固定 K 路径一致。
    let novice_user = user_elo.games < config.novice_game_threshold;
    let novice_word = word_elo.games < config.novice_game_threshold;
    let base_k_user = if novice_user {
        config.k_factor * config.novice_k_multiplier
    } else {
        config.k_factor
    };
    let base_k_word = if novice_word {
        config.k_factor * config.novice_k_multiplier * config.word_k_factor_ratio
    } else {
        config.k_factor * config.word_k_factor_ratio
    };

    // 动态乘子；novice 期下界钳 1.0（novice 倍率作下界）。
    let mut mu = dynamic_k_mult(*user_trend, config);
    let mut mw = dynamic_k_mult(*word_trend, config);
    if novice_user {
        mu = mu.max(1.0);
    }
    if novice_word {
        mw = mw.max(1.0);
    }
    let k_user = base_k_user * mu;
    let k_word = base_k_word * mw;

    user_elo.rating =
        (user_elo.rating + k_user * user_residual).clamp(config.min_elo, config.max_elo);
    word_elo.rating =
        (word_elo.rating + k_word * word_residual).clamp(config.min_elo, config.max_elo);

    user_elo.games += 1;
    word_elo.games += 1;

    // 趋势 EWMA 原地更新（残差证据累积；含本次）。
    let w = config.k_trend_weight;
    *user_trend = (1.0 - w) * *user_trend + w * user_residual;
    *word_trend = (1.0 - w) * *word_trend + w * word_residual;

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

    #[test]
    fn update_elo_with_trend_bit_exact_when_disabled() {
        // k_dynamic_enabled=false → 与 update_elo 逐位一致，且不触碰 trend。
        let config = EloConfig::default();
        assert!(!config.k_dynamic_enabled);
        let mut u1 = EloRating::default();
        let mut w1 = EloRating::default();
        let mut u2 = EloRating::default();
        let mut w2 = EloRating::default();
        let mut ut = 0.5;
        let mut wt = -0.5;
        for i in 0..15 {
            let correct = i % 3 != 0;
            update_elo(&mut u1, &mut w1, correct, &config);
            update_elo_with_trend(&mut u2, &mut w2, &mut ut, &mut wt, correct, &config);
        }
        assert_eq!(u1.rating, u2.rating);
        assert_eq!(w1.rating, w2.rating);
        assert_eq!(u1.games, u2.games);
        assert_eq!(ut, 0.5, "禁用时不应改 trend");
        assert_eq!(wt, -0.5);
    }

    #[test]
    fn dynamic_k_converges_faster_on_drift() {
        // 难度漂移：一名持续答对的用户。动态 K(同向残差累积→增K)应比固定 K 更快抬升 user 评分。
        let mut fixed = EloConfig::default();
        fixed.novice_game_threshold = 0; // 排除 novice 倍率干扰，纯比较动态项
        let mut dyn_cfg = fixed.clone();
        dyn_cfg.k_dynamic_enabled = true;

        let mut uf = EloRating::default();
        let mut wf = EloRating::default();
        let mut ud = EloRating::default();
        let mut wd = EloRating::default();
        let (mut udt, mut wdt) = (0.0, 0.0);
        for _ in 0..12 {
            update_elo(&mut uf, &mut wf, true, &fixed);
            update_elo_with_trend(&mut ud, &mut wd, &mut udt, &mut wdt, true, &dyn_cfg);
        }
        // 同向残差(全对)→ user_trend 正、word_trend 负，|trend| 增 → 动态 K 乘子 >1 → 抬升更快。
        assert!(ud.rating > uf.rating, "动态 user={} 固定 user={}", ud.rating, uf.rating);
        assert!(wd.rating < wf.rating, "动态 word={} 固定 word={}", wd.rating, wf.rating);
        assert!(udt > 0.0 && wdt < 0.0);
    }

    #[test]
    fn dynamic_k_damps_in_steady_state() {
        // 震荡(交替对错)→ trend≈0 → 乘子 = 1 - damp < 1 → 单步位移小于固定 K。
        let mut cfg = EloConfig::default();
        cfg.novice_game_threshold = 0;
        cfg.k_dynamic_enabled = true;
        // 先用交替序列把 trend 压到 ≈0
        let mut u = EloRating::default();
        let mut w = EloRating::default();
        let (mut ut, mut wt) = (0.0, 0.0);
        for i in 0..20 {
            update_elo_with_trend(&mut u, &mut w, &mut ut, &mut wt, i % 2 == 0, &cfg);
        }
        assert!(ut.abs() < 0.2, "震荡后 trend 应接近 0，实际 {ut}");
    }
}
