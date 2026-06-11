use serde::{Deserialize, Serialize};

use crate::amas::config::MemoryModelConfig;

/// AMAS v3: DSR (Difficulty-Stability-Retrievability) architecture
/// Uses FSRS-6 formulas (21-dim w, trainable forgetting-curve decay w[20])
/// for state transitions with AMAS-unique scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdmState {
    /// Stability: interval in days at which R = 90%.
    #[serde(default = "default_stability")]
    pub stability: f64,
    /// Difficulty: D ∈ [1, 10]. Higher = harder material.
    #[serde(default = "default_difficulty")]
    pub difficulty: f64,
    /// Backward-compatible alias: equals stability for downstream code.
    #[serde(default)]
    pub memory_strength: f64,
    pub last_review_at: Option<i64>,
    pub review_count: u32,
    // Legacy fields – kept for serde backward compat
    #[serde(default)]
    pub short_term_strength: f64,
    #[serde(default)]
    pub medium_term_strength: f64,
    #[serde(default)]
    pub long_term_strength: f64,
    #[serde(default)]
    pub consolidation: f64,
}

fn default_stability() -> f64 {
    0.4
}
fn default_difficulty() -> f64 {
    5.0
}

impl Default for MdmState {
    fn default() -> Self {
        Self {
            stability: 0.4,
            difficulty: 5.0,
            memory_strength: 0.0,
            short_term_strength: 0.0,
            medium_term_strength: 0.0,
            long_term_strength: 0.0,
            last_review_at: None,
            review_count: 0,
            consolidation: 0.0,
        }
    }
}

impl MdmState {
    /// Migrate legacy state: if stability is still the serde default (0.4)
    /// but memory_strength has a learned value, convert it to DSR stability.
    /// This prevents resetting historical memory progress on upgrade.
    pub fn migrate_legacy(&mut self) {
        const DEFAULT_STABILITY: f64 = 0.4;
        const STABILITY_BASE: f64 = 20.0;
        const HALF_LIFE_EPSILON: f64 = 0.3;
        const HALF_LIFE_POWER: f64 = 1.5;

        // Only migrate if stability was not explicitly set (still default)
        // AND the user has a non-trivial learned memory_strength
        if (self.stability - DEFAULT_STABILITY).abs() < 1e-9
            && self.memory_strength > 0.01
            && self.review_count > 0
        {
            // Old formula: stability_days = (memory_strength + epsilon)^power * base
            let migrated =
                (self.memory_strength + HALF_LIFE_EPSILON).powf(HALF_LIFE_POWER) * STABILITY_BASE;
            self.stability = migrated.clamp(0.01, 365.0);
        }
    }
}

/// 兼容入口（无证据计数）：以中性证据 streak=0/lapses=0（两腿 max(·,1)=1 ⇒ e^0=1 恒 no-op）
/// 委托 [`update_strength_with_evidence`]——即便 tau>0，本入口也保持精确冻结语义。
/// 生产路径（mastery.rs / benchmark_adapter.rs）必须走带证据形态，否则信任调度不生效。
pub fn update_strength(
    state: &mut MdmState,
    quality: f64,
    alpha: f64,
    now_ms: i64,
    config: &MemoryModelConfig,
) {
    update_strength_with_evidence(state, quality, alpha, 0, 0, now_ms, config);
}

/// Main DSR update function.
/// Uses FSRS-6 formulas for stability transitions.
///
/// quality: 0.0-1.0, mapped to FSRS grade:
///   quality <= 0.15 → Again(1), <= 0.5 → Hard(2), <= 0.85 → Good(3), else Easy(4)
///
/// 双腿信任调度证据（advance-before-update，由 mastery 层先记账再传入）：
/// - `correct_streak`：本次复习记账后的连击数（成功腿 τ_s 挂靠；失败清零→阻尼重启）
/// - `lapse_count`：含本次失败的累计 lapse 数（失败腿 τ_f 挂靠；首错 f=1 即 no-op）
#[allow(clippy::too_many_arguments)]
pub fn update_strength_with_evidence(
    state: &mut MdmState,
    quality: f64,
    alpha: f64,
    correct_streak: u32,
    lapse_count: u32,
    now_ms: i64,
    config: &MemoryModelConfig,
) {
    let quality = quality.clamp(0.0, 1.0);
    let alpha = alpha.clamp(0.0, 1.0);
    // Map quality to FSRS grade
    let grade: u32 = if quality <= 0.15 {
        1
    } else if quality <= 0.5 {
        2
    } else if quality <= 0.85 {
        3
    } else {
        4
    };

    if state.review_count == 0 {
        // First review: initial stability from w0-w3
        let g = (grade as usize).clamp(1, 4) - 1;
        state.stability = config.w[g];
        // Initial difficulty: D0(G) = w4 - e^{w5*(G-1)} + 1
        state.difficulty =
            (config.w[4] - (config.w[5] * (grade as f64 - 1.0)).exp() + 1.0).clamp(1.0, 10.0);
    } else {
        // Compute current R
        let r = recall_probability(state, now_ms, config);
        let prev_stability = state.stability.max(0.01);
        let prev_difficulty = state.difficulty;

        // Update difficulty: D' = D - w6*(G-3), then mean reversion
        let delta_d = -config.w[6] * (grade as f64 - 3.0);
        let d_prime = prev_difficulty + delta_d * (10.0 - prev_difficulty) / 9.0;
        let d0_4 = (config.w[4] - (config.w[5] * 3.0).exp() + 1.0).clamp(1.0, 10.0);
        let target_difficulty =
            (config.w[7] * d0_4 + (1.0 - config.w[7]) * d_prime).clamp(1.0, 10.0);

        let elapsed_days = state
            .last_review_at
            .map(|last| ((now_ms - last) as f64 / 86_400_000.0).max(0.0))
            .unwrap_or(1.0);

        let target_stability = if elapsed_days < 1.0 {
            // Same-day review: FSRS-6 short-term formula S' = S·e^{w17·(G-3+w18)}·S^{-w19}
            // （w19 饱和项：小 S 增长快、大 S 增长慢；G≥3 时强制 S'≥S）
            let grade_f = grade as f64;
            let exponent = (config.w[17] * (grade_f - 3.0 + config.w[18])).clamp(-20.0, 20.0);
            let saturation = prev_stability.powf(-config.w[19]);
            let s_short = (prev_stability * exponent.exp() * saturation).max(0.01);
            if grade >= 3 {
                s_short.max(prev_stability)
            } else {
                s_short
            }
        } else if grade >= 2 {
            // Successful recall: S'_r = S * (e^w8 * (11-D) * S^{-w9} * (e^{w10*(1-R)} - 1) * bonus + 1)
            let bonus = if grade == 2 {
                config.w[15]
            }
            // Hard
            else if grade == 4 {
                config.w[16]
            }
            // Easy
            else {
                1.0
            }; // Good
            let s_inc = (config.w[8].exp()
                * (11.0 - prev_difficulty)
                * prev_stability.powf(-config.w[9])
                * ((config.w[10] * (1.0 - r)).exp() - 1.0)
                * bonus)
                .max(0.0);
            (prev_stability * (s_inc + 1.0)).max(0.01)
        } else {
            // Forgetting (Again): S'_f = w11 * D^{-w12} * ((S+1)^w13 - 1) * e^{w14*(1-R)}
            (config.w[11]
                * prev_difficulty.powf(-config.w[12])
                * ((prev_stability + 1.0).powf(config.w[13]) - 1.0)
                * (config.w[14] * (1.0 - r)).exp())
            .clamp(0.01, prev_stability)
        };

        // 双腿信任调度（两旋钮 0=关闭即冻结语义；语义替换旧 count 挂靠，旧语义从未发布 ON）：
        // 成功腿（grade≥2，alphaRampTau=τ_s）：alpha_eff = 1-(1-alpha)·e^{-(k-1)/τ_s}，
        //   k=记账后 correct_streak（失败清零→阻尼重启；lapse 后同日成功 k=max(0,1)=1 即 no-op）
        // 失败腿（grade==1 Again，alphaLapseRampTau=τ_f）：alpha_eff = 1-(1-alpha)·e^{-(f-1)/τ_f}，
        //   f=含本次的累计 lapse（首错 f=1 ⇒ e^0=1 ⇒ alpha 不变 = 偶发失误保护；leech 加速压 S）
        // elif 结构：成功永不吃失败腿 ramp。运算结合序与 Python 镜像逐位一致（1e-9 对拍）
        let alpha = if grade >= 2 {
            if config.alpha_ramp_tau > 0.0 {
                let k = correct_streak.max(1) as f64;
                1.0 - (1.0 - alpha) * (-(k - 1.0) / config.alpha_ramp_tau).exp()
            } else {
                alpha
            }
        } else if config.alpha_lapse_ramp_tau > 0.0 {
            let f = lapse_count.max(1) as f64;
            1.0 - (1.0 - alpha) * (-(f - 1.0) / config.alpha_lapse_ramp_tau).exp()
        } else {
            alpha
        };

        state.difficulty =
            (prev_difficulty + (target_difficulty - prev_difficulty) * alpha).clamp(1.0, 10.0);
        // S 上限对齐 FSRS-6 参考实现（≈100 年），防极端 w 组合下幂运算溢出
        state.stability =
            (prev_stability + (target_stability - prev_stability) * alpha).clamp(0.01, 36_500.0);
    }

    // Sync backward-compatible fields
    state.memory_strength = state.stability;
    let composite_val = (state.stability / 30.0).clamp(0.0, 1.0);
    state.short_term_strength = composite_val;
    state.medium_term_strength = composite_val;
    state.long_term_strength = composite_val;
    state.consolidation = (1.5 - state.difficulty / 10.0).clamp(0.0, 1.0);

    state.review_count += 1;
    state.last_review_at = Some(now_ms);
}

/// B40: Compute adaptive desired_retention based on various factors
pub fn adaptive_desired_retention(
    base_retention: f64,
    accuracy: f64,
    fatigue: f64,
    motivation: f64,
    config: &MemoryModelConfig,
) -> f64 {
    let mut retention = base_retention;

    let sigmoid = |x: f64| 1.0 / (1.0 + (-x).exp());

    retention += config.high_accuracy_retention_boost
        * sigmoid((accuracy - config.high_accuracy_threshold) * 10.0);
    retention -= config.high_fatigue_retention_drop
        * sigmoid((fatigue - config.high_fatigue_threshold) * 10.0);
    retention -= config.low_motivation_retention_drop
        * sigmoid((config.low_motivation_threshold - motivation) * 10.0);

    retention.clamp(config.retention_min, config.retention_max)
}

/// Backward-compatible composite strength (normalized to [0,1])
pub fn composite_strength(state: &MdmState, _config: &MemoryModelConfig) -> f64 {
    (state.stability / 30.0).clamp(0.0, 1.0)
}

/// FSRS-6 power-law forgetting curve with asymptote:
/// R(t,S) = floor + (1-floor) * (1 + factor * t/S)^(-decay)
/// decay = w[20] (trainable), factor = 0.9^(-1/decay) - 1 so that R(S,S)=0.9 (floor=0)
pub fn recall_probability(state: &MdmState, now_ms: i64, config: &MemoryModelConfig) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = ((now_ms - last) as f64 / 86_400_000.0).max(0.0);
            let s = state.stability.max(0.01);
            let power_law = (1.0 + config.curve_factor() * elapsed_days / s)
                .powf(-config.curve_decay());
            let floor = config.forgetting_curve_floor;
            (floor + (1.0 - floor) * power_law).clamp(0.0, 1.0)
        }
    }
}

/// Interval: solve R = floor + (1-floor) * (1 + factor*t/S)^(-decay) for t
pub fn compute_interval(
    state: &MdmState,
    target_recall: f64,
    interval_scale: f64,
    config: &MemoryModelConfig,
) -> i64 {
    let s = state.stability.max(0.01);
    let floor = config.forgetting_curve_floor;
    let adjusted_target = ((target_recall - floor) / (1.0 - floor).max(1e-9)).clamp(1e-6, 1.0);
    let interval_days = s / config.curve_factor()
        * (adjusted_target.powf(-1.0 / config.curve_decay()) - 1.0);
    let interval_secs = interval_days * 86400.0;
    ((interval_secs * interval_scale.max(0.1)).min(config.max_interval_days * 86400.0) as i64)
        .max(config.min_interval_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_is_bounded_and_monotonic() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let now_ms = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.8, 0.3, now_ms, &config);
        let t1 = state.last_review_at.unwrap() + 1000;
        let t2 = state.last_review_at.unwrap() + 5000;
        let p1 = recall_probability(&state, t1, &config);
        let p2 = recall_probability(&state, t2, &config);
        assert!((0.0..=1.0).contains(&p1));
        assert!((0.0..=1.0).contains(&p2));
        assert!(p2 <= p1);
    }

    #[test]
    fn composite_strength_moves_up_after_good_quality() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let before = composite_strength(&state, &config);
        let now_ms = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.9, 0.3, now_ms, &config);
        let after = composite_strength(&state, &config);
        assert!(after >= before);
    }

    #[test]
    fn stability_grows_after_successful_recall() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let now = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.9, 0.3, now, &config);
        let s1 = state.stability;
        let later = now + 86_400_000;
        update_strength(&mut state, 0.9, 0.3, later, &config);
        let s2 = state.stability;
        assert!(
            s2 > s1,
            "Stability should grow after successful recall: {} > {}",
            s2,
            s1
        );
    }

    #[test]
    fn stability_decreases_after_forgetting() {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        let now = chrono::Utc::now().timestamp_millis();
        update_strength(&mut state, 0.9, 0.3, now, &config);
        let s1 = state.stability;
        let later = now + 86_400_000;
        update_strength(&mut state, 0.0, 0.3, later, &config);
        let s2 = state.stability;
        assert!(
            s2 < s1,
            "Stability should decrease after forgetting: {} < {}",
            s2,
            s1
        );
    }

    #[test]
    fn larger_alpha_produces_stronger_post_review_update() {
        let config = MemoryModelConfig::default();
        let now = chrono::Utc::now().timestamp_millis();
        let later = now + 86_400_000;

        let mut low_alpha = MdmState::default();
        update_strength(&mut low_alpha, 0.9, 0.3, now, &config);
        let mut high_alpha = low_alpha.clone();

        update_strength(&mut low_alpha, 0.9, 0.1, later, &config);
        update_strength(&mut high_alpha, 0.9, 0.9, later, &config);

        assert!(high_alpha.stability > low_alpha.stability);
    }

    const DAY_MS: i64 = 86_400_000;

    fn dual_config(tau_s: f64, tau_f: f64) -> MemoryModelConfig {
        let mut config = MemoryModelConfig::default();
        config.alpha_ramp_tau = tau_s;
        config.alpha_lapse_ramp_tau = tau_f;
        config
    }

    /// 经过一次成功平滑复习的种子状态（review_count=2，可进平滑分支）
    fn seeded_state(now: i64, config: &MemoryModelConfig) -> MdmState {
        let mut state = MdmState::default();
        update_strength_with_evidence(&mut state, 0.7, 0.3, 1, 0, now, config);
        update_strength_with_evidence(&mut state, 0.7, 0.3, 2, 0, now + DAY_MS, config);
        state
    }

    /// 两旋钮 tau=0（serde/Default 默认，分支不进）必须与冻结语义逐位一致；
    /// tau>0 在 k=1 / f=1（exp(-0)=1）对二进制可精确表示的 alpha 位级回原值
    /// （非二进制精确 alpha 如 0.33 时 1-(1-α) round-trip 有 1 ULP 误差，属 IEEE 固有）
    #[test]
    fn dual_trust_tau_zero_and_unit_evidence_keep_frozen_semantics() {
        let base_config = MemoryModelConfig::default();
        assert_eq!(base_config.alpha_ramp_tau, 0.0);
        assert_eq!(base_config.alpha_lapse_ramp_tau, 0.0);
        let zero_config = dual_config(0.0, 0.0);
        let dual = dual_config(5.0, 5.0);

        let now = chrono::Utc::now().timestamp_millis();
        let mut frozen = MdmState::default();
        let mut zeroed = MdmState::default();
        let mut ramped = MdmState::default();
        // 首评（review_count==0 分支，alpha 未用）+ 成功 k=1 + 失败 f=1：
        // 两腿在单位证据处都必须 no-op；alpha 取 dyadic 0.25（1-0.25=0.75 精确）
        for (step, quality, streak, lapses) in [(0i64, 0.7, 1u32, 0u32), (1, 0.7, 2, 0), (2, 0.0, 0, 1)]
        {
            let at = now + step * DAY_MS;
            update_strength_with_evidence(&mut frozen, quality, 0.25, streak, lapses, at, &base_config);
            update_strength_with_evidence(&mut zeroed, quality, 0.25, streak, lapses, at, &zero_config);
            // k=2 处成功腿会 ramp，故 dual 配置只喂 k≤1/f≤1 的证据验证单位证据 no-op
            let unit_streak = streak.min(1);
            update_strength_with_evidence(&mut ramped, quality, 0.25, unit_streak, lapses, at, &dual);
        }
        assert_eq!(frozen.stability, zeroed.stability);
        assert_eq!(frozen.difficulty, zeroed.difficulty);
        assert_eq!(frozen.stability, ramped.stability);
        assert_eq!(frozen.difficulty, ramped.difficulty);
    }

    /// 兼容入口 update_strength（中性证据 0/0）：即便两 tau>0 也保持冻结语义
    #[test]
    fn legacy_entry_stays_frozen_under_positive_tau() {
        let base_config = MemoryModelConfig::default();
        let dual = dual_config(3.0, 6.0);
        let now = chrono::Utc::now().timestamp_millis();

        let mut frozen = MdmState::default();
        let mut ramped = MdmState::default();
        for (step, quality) in [(0i64, 0.7), (1, 0.7), (2, 0.0), (3, 0.7)] {
            let at = now + step * DAY_MS;
            update_strength(&mut frozen, quality, 0.3, at, &base_config);
            update_strength(&mut ramped, quality, 0.3, at, &dual);
        }
        assert_eq!(frozen.stability, ramped.stability);
        assert_eq!(frozen.difficulty, ramped.difficulty);
    }

    /// 成功腿隐式 alpha_eff 对闭式公式：同一前置状态分别以 tau=0 / τ_s>0 更新一步，
    /// (S'_ramp − prev)/(S'_frozen − prev) = alpha_eff/alpha；并断言随 k 单调递增
    #[test]
    fn success_leg_implied_alpha_matches_closed_form_and_grows_with_streak() {
        let base_config = MemoryModelConfig::default();
        let tau = 5.0;
        let ramped_config = dual_config(tau, 0.0);
        let alpha = 0.3;

        let now = chrono::Utc::now().timestamp_millis();
        let mut state = MdmState::default();
        update_strength_with_evidence(&mut state, 0.7, alpha, 1, 0, now, &base_config);

        let mut prev_eff = 0.0;
        for step in 1..=6i64 {
            let at = now + step * DAY_MS;
            let streak = (step + 1) as u32; // 连续成功：记账后 k = step+1
            let k = streak as f64;
            let mut frozen = state.clone();
            let mut ramped = state.clone();
            update_strength_with_evidence(&mut frozen, 0.7, alpha, streak, 0, at, &base_config);
            update_strength_with_evidence(&mut ramped, 0.7, alpha, streak, 0, at, &ramped_config);

            let expected_eff = 1.0 - (1.0 - alpha) * (-(k - 1.0) / tau).exp();
            let mut implied_eff = expected_eff;
            for (got, base, prev) in [
                (ramped.stability, frozen.stability, state.stability),
                (ramped.difficulty, frozen.difficulty, state.difficulty),
            ] {
                if (base - prev).abs() <= 1e-9 {
                    continue; // 平滑增量过小时比值数值不稳，跳过
                }
                implied_eff = alpha * (got - prev) / (base - prev);
                assert!(
                    (implied_eff - expected_eff).abs() <= 1e-9 * expected_eff.max(1.0),
                    "step {step}: implied {implied_eff} vs expected {expected_eff}"
                );
            }
            assert!(
                implied_eff > prev_eff,
                "step {step}: alpha_eff 应随 k 单调递增 ({implied_eff} <= {prev_eff})"
            );
            prev_eff = implied_eff;
            state = frozen;
        }
    }

    /// 成功腿挂靠 streak 而非累计复习数：失败清零后（k 回 1）ramp 必须完全重启
    #[test]
    fn success_leg_resets_with_streak_not_review_count() {
        let base_config = MemoryModelConfig::default();
        let ramped_config = dual_config(3.0, 0.0);
        let now = chrono::Utc::now().timestamp_millis();

        // 长历史（review_count=4）但失败清零后首个成功 k=1 → e^0=1 ⇒ 与冻结逐位一致；
        // 位级断言须用 dyadic alpha 0.25（1-(1-α) round-trip 对 0.3 有 1 ULP 误差，IEEE 固有）
        let mut frozen = seeded_state(now, &base_config);
        update_strength_with_evidence(&mut frozen, 0.0, 0.25, 0, 1, now + 2 * DAY_MS, &base_config);
        let mut ramped = frozen.clone();
        update_strength_with_evidence(&mut frozen, 0.7, 0.25, 1, 1, now + 3 * DAY_MS, &base_config);
        update_strength_with_evidence(&mut ramped, 0.7, 0.25, 1, 1, now + 3 * DAY_MS, &ramped_config);
        assert_eq!(frozen.stability, ramped.stability);
        assert_eq!(frozen.difficulty, ramped.difficulty);

        // 同一前置状态、同 review_count，k=2 必须发散（判别：挂靠的是 streak）
        let mut frozen2 = frozen.clone();
        let mut ramped2 = frozen;
        update_strength_with_evidence(&mut frozen2, 0.7, 0.25, 2, 1, now + 4 * DAY_MS, &base_config);
        update_strength_with_evidence(&mut ramped2, 0.7, 0.25, 2, 1, now + 4 * DAY_MS, &ramped_config);
        assert_ne!(frozen2.stability, ramped2.stability);
    }

    /// 失败腿：首错 f=1 逐位 no-op（偶发失误保护）；重复失败 f≥2 必须实化
    /// （隐式 alpha_eff 对闭式公式 1-(1-α)e^{-(f-1)/τ_f}）
    #[test]
    fn lapse_leg_first_failure_noop_then_realizes_on_repeats() {
        let base_config = MemoryModelConfig::default();
        let tau_f = 6.0;
        let ramped_config = dual_config(0.0, tau_f);
        // dyadic alpha：f=1 位级断言要求 1-(1-α) round-trip 精确（0.3 有 1 ULP 误差）
        let alpha = 0.25;
        let now = chrono::Utc::now().timestamp_millis();

        // f=1：首错保护，必须与冻结语义逐位一致
        let seed = seeded_state(now, &base_config);
        let mut frozen = seed.clone();
        let mut ramped = seed.clone();
        update_strength_with_evidence(&mut frozen, 0.0, alpha, 0, 1, now + 3 * DAY_MS, &base_config);
        update_strength_with_evidence(&mut ramped, 0.0, alpha, 0, 1, now + 3 * DAY_MS, &ramped_config);
        assert_eq!(frozen.stability, ramped.stability);
        assert_eq!(frozen.difficulty, ramped.difficulty);

        // f=2..4：alpha_eff 按闭式公式实化且随 f 单调递增（加速压 S）
        let mut prev_eff = 0.0;
        let mut state = frozen;
        for f in 2u32..=4 {
            let at = now + (2 + f as i64) * DAY_MS;
            let mut frozen_next = state.clone();
            let mut ramped_next = state.clone();
            update_strength_with_evidence(&mut frozen_next, 0.0, alpha, 0, f, at, &base_config);
            update_strength_with_evidence(&mut ramped_next, 0.0, alpha, 0, f, at, &ramped_config);

            let expected_eff = 1.0 - (1.0 - alpha) * (-((f as f64) - 1.0) / tau_f).exp();
            let mut implied_eff = expected_eff;
            for (got, base, prev) in [
                (ramped_next.stability, frozen_next.stability, state.stability),
                (ramped_next.difficulty, frozen_next.difficulty, state.difficulty),
            ] {
                if (base - prev).abs() <= 1e-9 {
                    continue;
                }
                implied_eff = alpha * (got - prev) / (base - prev);
                assert!(
                    (implied_eff - expected_eff).abs() <= 1e-9 * expected_eff.max(1.0),
                    "f={f}: implied {implied_eff} vs expected {expected_eff}"
                );
            }
            assert!(
                implied_eff > prev_eff,
                "f={f}: alpha_eff 应随 f 单调递增 ({implied_eff} <= {prev_eff})"
            );
            prev_eff = implied_eff;
            state = frozen_next;
        }
    }

    /// elif 互斥（成功侧）：grade≥2 且 τ_s=0 时，即便 τ_f>0、lapse 数很大也不得吃失败腿 ramp
    #[test]
    fn success_never_eats_lapse_leg() {
        let base_config = MemoryModelConfig::default();
        let lapse_only = dual_config(0.0, 2.0);
        let now = chrono::Utc::now().timestamp_millis();

        let seed = seeded_state(now, &base_config);
        // quality 0.4→Hard(2)、0.7→Good(3)、0.95→Easy(4) 全成功档 + 大 lapse 证据
        for quality in [0.4, 0.7, 0.95] {
            let mut frozen = seed.clone();
            let mut ramped = seed.clone();
            update_strength_with_evidence(&mut frozen, quality, 0.3, 1, 9, now + 3 * DAY_MS, &base_config);
            update_strength_with_evidence(&mut ramped, quality, 0.3, 1, 9, now + 3 * DAY_MS, &lapse_only);
            assert_eq!(frozen.stability, ramped.stability, "quality {quality}");
            assert_eq!(frozen.difficulty, ramped.difficulty, "quality {quality}");
        }
    }

    /// 互斥（失败侧）：grade==1（Again）豁免成功腿——τ_s>0、streak 证据大也不得 ramp
    #[test]
    fn again_grade_exempt_from_success_leg() {
        let base_config = MemoryModelConfig::default();
        let success_only = dual_config(2.5, 0.0);
        let now = chrono::Utc::now().timestamp_millis();

        let seed = seeded_state(now, &base_config);
        let mut frozen = seed.clone();
        let mut ramped = seed;
        // 失败（quality 0.0 → Again）：即便传入大 streak 证据也必须保持原阻尼
        update_strength_with_evidence(&mut frozen, 0.0, 0.3, 5, 1, now + 3 * DAY_MS, &base_config);
        update_strength_with_evidence(&mut ramped, 0.0, 0.3, 5, 1, now + 3 * DAY_MS, &success_only);
        assert_eq!(frozen.stability, ramped.stability);
        assert_eq!(frozen.difficulty, ramped.difficulty);
    }

    /// quality 0.4→Hard(2)、0.95→Easy(4) 均进成功腿 ramp
    /// （bench 域只打 Good/Again，此处直测生产域 4 档 grade 门控）
    #[test]
    fn success_leg_engages_hard_and_easy_grades() {
        let base_config = MemoryModelConfig::default();
        let ramped_config = dual_config(2.5, 0.0);
        let now = chrono::Utc::now().timestamp_millis();

        for quality in [0.4, 0.95] {
            let seed = seeded_state(now, &base_config);
            // k=3：alpha_eff != alpha → 状态必须发散
            let mut frozen = seed.clone();
            let mut ramped = seed;
            update_strength_with_evidence(&mut frozen, quality, 0.3, 3, 0, now + 2 * DAY_MS, &base_config);
            update_strength_with_evidence(&mut ramped, quality, 0.3, 3, 0, now + 2 * DAY_MS, &ramped_config);
            assert_ne!(frozen.stability, ramped.stability, "quality {quality}");
        }
    }
}
