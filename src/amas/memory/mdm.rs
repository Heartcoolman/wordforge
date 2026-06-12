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
    let mut grade: u32 = if quality <= 0.15 {
        1
    } else if quality <= 0.5 {
        2
    } else if quality <= 0.85 {
        3
    } else {
        4
    };
    // GSP 成功成绩带（gsp_success_grade，契约 GSP_SPEC §3.5）：
    // ==4 时二元成功映射到 Easy(4)，进入 FSRS-6 faithful 状态路径（D 先更新 / S 用新 D /
    // 无同日特化）。失败（grade==1 Again）恒不变。==3（默认）逐位 legacy 不变。
    // 仅在合法成功成绩域 {3,4} 生效；其余值（不应出现，validate 已拒）按 3 处理不重映射。
    let fsrs6_faithful = config.gsp_success_grade == 4;
    if fsrs6_faithful && grade >= 2 {
        grade = 4;
    }

    if state.review_count == 0 {
        // First review: initial stability from w0-w3
        let g = (grade as usize).clamp(1, 4) - 1;
        state.stability = config.w[g];
        // Initial difficulty: D0(G) = w4 - e^{w5*(G-1)} + 1
        state.difficulty =
            (config.w[4] - (config.w[5] * (grade as f64 - 1.0)).exp() + 1.0).clamp(1.0, 10.0);
    } else if fsrs6_faithful {
        // —— FSRS-6 faithful 状态路径（gsp_success_grade==4，契约 GSP_SPEC §3.5）——
        // 逐位等价 Python FSRS6MirrorState / WordforgeMirrorState 的 success_grade==4 分支：
        // ① 难度先更新：self.difficulty 先算出，S 公式用更新后的 D（而非 prev_D）；
        // ② mean-reversion 锚 D0(Easy=4)（d_target 不预夹，仅最终 D 夹 [1,10]）；
        // ③ 无同日特化分支（elapsed<1 仍走常规成功/遗忘公式）；
        // ④ 无 alpha 平滑：直接赋值 target（v5 候选 alpha 钉死 1.0，平滑本为 no-op）。
        let r = recall_probability(state, now_ms, config);
        let prev_difficulty = state.difficulty;
        // D 先更新（mean-reversion 目标锚 D0(Easy=4)）
        let delta_d = -config.w[6] * (grade as f64 - 3.0);
        let d_prime = prev_difficulty + delta_d * (10.0 - prev_difficulty) / 9.0;
        let d_target = config.w[4] - (config.w[5] * (4.0 - 1.0)).exp() + 1.0;
        state.difficulty =
            (config.w[7] * d_target + (1.0 - config.w[7]) * d_prime).clamp(1.0, 10.0);
        // S 公式使用更新后的 self.difficulty
        let prev_stability = state.stability.max(0.01);
        if grade >= 2 {
            let hard_penalty = if grade == 2 { config.w[15] } else { 1.0 };
            let easy_bonus = if grade == 4 { config.w[16] } else { 1.0 };
            let s_inc = (config.w[8].exp()
                * (11.0 - state.difficulty)
                * prev_stability.powf(-config.w[9])
                * ((config.w[10] * (1.0 - r)).exp() - 1.0)
                * hard_penalty
                * easy_bonus)
                .max(0.0);
            state.stability = (prev_stability * (s_inc + 1.0)).clamp(0.01, 36_500.0);
        } else {
            // post-lapse：S'_f = w11·D^{-w12}·((S+1)^w13−1)·e^{w14(1-R)}，夹至 [0.01, prev_S]
            let s_lapse = config.w[11]
                * state.difficulty.powf(-config.w[12])
                * ((prev_stability + 1.0).powf(config.w[13]) - 1.0)
                * (config.w[14] * (1.0 - r)).exp();
            state.stability = s_lapse.clamp(0.01, prev_stability);
        }
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

/// GSP 调度策略头是否激活：任一旋钮非关闭值即激活。全关时生产走精确旧 compute_interval 路径
/// （秒级管线，bit-exact legacy）；任一旋钮开启时切换到 GSP 天级 head（GSP_SPEC §3）。
/// 注：gsp_success_grade 是 state-update 旋钮（影响 S/D），不影响调度 head 是否激活。
pub fn gsp_schedule_active(config: &MemoryModelConfig) -> bool {
    config.gsp_interval_cap_days > 0.0
        || config.gsp_graduation_streak > 0
        || config.gsp_maturity_band_days > 0.0
        || config.gsp_interval_fuzz > 0.0
}

/// GSP base 区间（天，整数；契约 GSP_SPEC §3 步骤 1–2）：解曲线 R(t,S)=target_recall，
/// 顶侧夹 max_interval_days(=90)，再 max(1, ceil)。逐位等价 Python
/// `WordforgeMirrorState.interval_days()`（mdm.rs compute_interval 的天级、未乘 scale 形态）。
/// target_recall 由调用方传入（band>0 时为 banded retention，否则 desired_retention）。
pub fn compute_interval_base_days(
    state: &MdmState,
    target_recall: f64,
    config: &MemoryModelConfig,
) -> i64 {
    let s = state.stability.max(0.01);
    let floor = config.forgetting_curve_floor;
    let adjusted_target = ((target_recall - floor) / (1.0 - floor).max(1e-9)).clamp(1e-6, 1.0);
    let days = s / config.curve_factor()
        * (adjusted_target.powf(-1.0 / config.curve_decay()) - 1.0);
    let capped = days.min(config.max_interval_days);
    (capped.ceil() as i64).max(1)
}

/// GSP 成熟度分带（契约 GSP_SPEC §3.1）：band>0 时返回替换 desired_retention 的目标保持率；
/// 关闭时返回 None。按当前 stability 与 band 的关系选 young/mature（< band 用 young，
/// >= band 用 mature），各自夹至曲线合法域 [1e-6, 1.0]。仅区间求解口径，预测/recall 不受影响。
pub fn gsp_banded_retention(state: &MdmState, config: &MemoryModelConfig) -> Option<f64> {
    if config.gsp_maturity_band_days <= 0.0 {
        return None;
    }
    let target = if state.stability < config.gsp_maturity_band_days {
        config.gsp_young_retention
    } else {
        config.gsp_mature_retention
    };
    Some(target.clamp(1e-6, 1.0))
}

/// GSP_SPEC §7.2 确定性抖动量 u ∈ [-1, 1)，由 in-state 量派生（时间不变、身份无关）：
///   h = stability·12.9898 + review_count·78.233（先各自乘、再加，f64 同序）
///   f = h − floor(h)；u = 2f − 1
/// 常数取自经典 GLSL fract(sin(dot(...))) hash 家族但去 sin（保证 Python↔Rust 可移植）。
fn gsp_fuzz_u(stability: f64, review_count: u32) -> f64 {
    let h = stability * 12.9898 + review_count as f64 * 78.233;
    let f = h - h.floor();
    2.0 * f - 1.0
}

/// banker's rounding（round-half-to-even），对齐 Python 3 `round` 语义（GSP_SPEC §3 取整）。
/// Rust `f64::round` 为 round-half-away-from-zero，会在 .5 边界与 Python 差 1。
fn round_half_to_even(x: f64) -> f64 {
    let r = x.round();
    if (x - x.floor() - 0.5).abs() < f64::EPSILON {
        // 恰好 .5：取最近偶数
        let f = x.floor();
        if (f as i64) % 2 == 0 {
            f
        } else {
            f + 1.0
        }
    } else {
        r
    }
}

/// GSP 调度策略头（契约 GSP_SPEC §3 步骤 3–6 + §7）：在已含 banded-retention 求解的
/// **整数 base 天**（ceil 后）之上，依次施加 interval_scale → 毕业下限 → 区间帽 → 抖动 → 取整。
/// 全链路时间不变（仅读 correct_streak / stability / review_count，不读模拟日/视界）。
/// 默认全关时（cap=0, streak=0, fuzz=0, band=0）逐位等价旧语义 base_days·scale 后取整。
///
/// 返回**天数**（>=1 整数）。调用方按需 ×86400 转秒（生产 next_review_interval_secs）。
pub fn gsp_schedule_days(
    base_days_int: i64,
    interval_scale: f64,
    correct_streak: u32,
    state: &MdmState,
    config: &MemoryModelConfig,
) -> i64 {
    // 步骤 3：ensemble interval_scale（保真对齐 ensemble.rs：.max(0.1)，无上界、无分带）
    let mut scaled_days = (base_days_int as f64 * interval_scale.max(0.1)).max(1.0);

    // 步骤 4：毕业下限（scale 之后、cap 之前）
    let graduated = config.gsp_graduation_streak > 0
        && correct_streak >= config.gsp_graduation_streak;
    if graduated {
        scaled_days = scaled_days.max(config.gsp_graduation_floor_days);
    }

    // 步骤 5：区间帽 min(90, gspCap)，与既有 90 天硬帽复合
    let mut cap = config.max_interval_days;
    if config.gsp_interval_cap_days > 0.0 {
        cap = cap.min(config.gsp_interval_cap_days);
    }
    scaled_days = scaled_days.min(cap);

    // 步骤 5.5：区间抖动（cap 之后、取整之前；默认关闭即 no-op）
    if config.gsp_interval_fuzz > 0.0 {
        let u = gsp_fuzz_u(state.stability, state.review_count);
        let fuzzed = scaled_days * (1.0 + config.gsp_interval_fuzz * u);
        // 顶侧夹 fuzz_cap = min(90, cap·(1+fuzz))：允许抖动把 cap-pinned 词推到 cap 之上错峰，
        // 但绝不越 90 天硬帽。底侧夹 lo：毕业词夹 floor（不破 mastered 定义），非毕业词夹 1。
        let fuzz_cap = config
            .max_interval_days
            .min(cap * (1.0 + config.gsp_interval_fuzz));
        let lo = if graduated {
            config.gsp_graduation_floor_days
        } else {
            1.0
        };
        scaled_days = fuzzed.clamp(lo, fuzz_cap);
    }

    // 步骤 6：取整 + 底侧夹（banker's rounding 对齐 Python round）
    (round_half_to_even(scaled_days) as i64).max(1)
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

    // ===== GSP 调度策略头（契约 GSP_SPEC）=====

    fn gsp_config() -> MemoryModelConfig {
        // FSRS-6 公版 w（与 amas_config.toml F1 船值一致），其余 default
        let mut c = MemoryModelConfig::default();
        c.w = crate::amas::config::default_w();
        c.base_desired_retention = 0.85;
        c
    }

    /// gsp_success_grade 默认 3：grade>=2 不重映射，逐位 = legacy 路径。
    #[test]
    fn gsp_success_grade_default_3_is_legacy() {
        let config = gsp_config();
        assert_eq!(config.gsp_success_grade, 3);
        let now = chrono::Utc::now().timestamp_millis();
        let mut s = MdmState::default();
        update_strength_with_evidence(&mut s, 0.7, 1.0, 1, 0, now, &config);
        // grade=3 首评 → S0 = w[2]
        assert!((s.stability - config.w[2]).abs() < 1e-12);
    }

    /// gsp_success_grade=4：二元成功映射 Easy(4)，首评 S0=w[3]，进入 FSRS-6 faithful 路径。
    /// 逐位等价手算 FSRS6MirrorState（D 先更新 / S 用新 D / 无同日特化）。
    #[test]
    fn gsp_success_grade_4_fsrs6_faithful_matches_hand_calc() {
        let mut config = gsp_config();
        config.gsp_success_grade = 4;
        config.alpha_min = 1.0;
        config.alpha_max = 1.0;
        let w = &config.w;
        let now = chrono::Utc::now().timestamp_millis();
        let mut s = MdmState::default();
        // 首评成功（grade→4）：S0=w[3]，D0=w4-exp(w5*3)+1
        update_strength_with_evidence(&mut s, 0.7, 1.0, 1, 0, now, &config);
        assert!((s.stability - w[3]).abs() < 1e-12, "S0 应取 w[3]（Easy）");
        let d0 = (w[4] - (w[5] * 3.0).exp() + 1.0).clamp(1.0, 10.0);
        assert!((s.difficulty - d0).abs() < 1e-12);

        // 第二次成功（跨日）：手算 FSRS-6 faithful（D 先更新，S 用新 D）
        let prev_s = s.stability;
        let prev_d = s.difficulty;
        let at = now + DAY_MS;
        let r = recall_probability(&s, at, &config);
        let delta_d = -w[6] * (4.0 - 3.0);
        let d_prime = prev_d + delta_d * (10.0 - prev_d) / 9.0;
        let d_target = w[4] - (w[5] * 3.0).exp() + 1.0;
        let exp_d = (w[7] * d_target + (1.0 - w[7]) * d_prime).clamp(1.0, 10.0);
        let s_inc = (w[8].exp()
            * (11.0 - exp_d)
            * prev_s.powf(-w[9])
            * ((w[10] * (1.0 - r)).exp() - 1.0)
            * w[16])
            .max(0.0);
        let exp_s = (prev_s * (s_inc + 1.0)).clamp(0.01, 36_500.0);
        update_strength_with_evidence(&mut s, 0.7, 1.0, 2, 0, at, &config);
        assert!((s.difficulty - exp_d).abs() < 1e-9, "D: {} vs {}", s.difficulty, exp_d);
        assert!((s.stability - exp_s).abs() < 1e-9, "S: {} vs {}", s.stability, exp_s);
    }

    /// GSP 全关时调度 head 不激活，走旧 compute_interval 路径（bit-exact legacy）。
    #[test]
    fn gsp_off_not_active() {
        let config = gsp_config();
        assert!(!gsp_schedule_active(&config));
    }

    /// 任一 GSP 调度旋钮开启即激活；gsp_success_grade 不影响 head 激活（它是 state 旋钮）。
    #[test]
    fn gsp_schedule_active_gated_by_scheduling_knobs() {
        let mut c = gsp_config();
        c.gsp_success_grade = 4; // 仅 state 旋钮，不激活 head
        assert!(!gsp_schedule_active(&c));
        c.gsp_interval_cap_days = 40.0;
        assert!(gsp_schedule_active(&c));
        let mut c = gsp_config();
        c.gsp_graduation_streak = 2;
        assert!(gsp_schedule_active(&c));
        let mut c = gsp_config();
        c.gsp_maturity_band_days = 14.0;
        assert!(gsp_schedule_active(&c));
        let mut c = gsp_config();
        c.gsp_interval_fuzz = 0.1;
        assert!(gsp_schedule_active(&c));
    }

    /// 毕业下限：streak>=k 时区间弹到 floor；streak<k 时不施加。
    #[test]
    fn gsp_graduation_floor_triggers_at_k() {
        let mut config = gsp_config();
        config.gsp_graduation_streak = 3;
        config.gsp_graduation_floor_days = 30.0;
        let mut s = MdmState::default();
        s.stability = 5.0; // 低 S → base 区间 < 30
        s.review_count = 3;
        s.last_review_at = Some(0);
        let base = compute_interval_base_days(&s, 0.85, &config);
        assert!(base < 30, "构造的 base 应 < 30，实际 {base}");
        // streak=2 < k：不毕业，区间 = base
        let d2 = gsp_schedule_days(base, 1.0, 2, &s, &config);
        assert!(d2 < 30);
        // streak=3 == k：毕业，区间弹到 >=30
        let d3 = gsp_schedule_days(base, 1.0, 3, &s, &config);
        assert!(d3 >= 30, "streak>=k 应弹到 floor，实际 {d3}");
    }

    /// 区间帽与 90 天硬帽复合 min(90, cap)。
    #[test]
    fn gsp_cap_composes_with_90() {
        let mut config = gsp_config();
        let mut s = MdmState::default();
        s.stability = 500.0; // 高 S → base 撞 90
        s.review_count = 10;
        s.last_review_at = Some(0);
        let base = compute_interval_base_days(&s, 0.85, &config);
        assert_eq!(base, 90, "高 S 应撞 90 帽");
        // cap=45：压到 45
        config.gsp_interval_cap_days = 45.0;
        assert_eq!(gsp_schedule_days(base, 1.0, 0, &s, &config), 45);
        // cap=120 > 90：min(90,120)=90，no-op
        config.gsp_interval_cap_days = 120.0;
        assert_eq!(gsp_schedule_days(base, 1.0, 0, &s, &config), 90);
    }

    /// fuzz 确定性 + 同输入恒等 + u∈[-1,1)。
    #[test]
    fn gsp_fuzz_u_deterministic_and_bounded() {
        for (stab, rc) in [(1.5, 3u32), (37.2, 10), (90.0, 25), (0.4, 1), (12.34, 7)] {
            let u = gsp_fuzz_u(stab, rc);
            assert_eq!(u, gsp_fuzz_u(stab, rc));
            assert!((-1.0..1.0).contains(&u), "u 越界: {u}");
        }
    }

    /// fuzz 不把已毕业词拉到 floor 之下（mastered 定义不破），不越 90 硬帽。
    #[test]
    fn gsp_fuzz_preserves_graduated_floor_and_90_cap() {
        let mut config = gsp_config();
        config.gsp_interval_cap_days = 40.0;
        config.gsp_graduation_streak = 2;
        config.gsp_graduation_floor_days = 30.0;
        config.gsp_interval_fuzz = 0.20;
        // 扫多组 state 使 u 可能为负，断言毕业词区间恒 >=30 且 <=90
        for (stab, rc) in [(40.0, 5u32), (35.7, 8), (50.1, 12), (38.2, 3), (45.9, 20)] {
            let mut s = MdmState::default();
            s.stability = stab;
            s.review_count = rc;
            s.last_review_at = Some(0);
            let base = compute_interval_base_days(&s, 0.85, &config);
            let d = gsp_schedule_days(base, 1.0, 2, &s, &config); // streak=2 → 毕业
            assert!(d >= 30, "毕业词被 fuzz 击穿 floor: {d}");
            assert!(d <= 90, "fuzz 越 90 硬帽: {d}");
        }
    }

    /// banded retention：S 跨 band 时目标保持率切换（< band → young，>= band → mature）。
    #[test]
    fn gsp_banded_retention_switches_at_band() {
        let mut config = gsp_config();
        config.gsp_maturity_band_days = 20.0;
        config.gsp_young_retention = 0.95;
        config.gsp_mature_retention = 0.80;
        let mut s = MdmState::default();
        s.stability = 19.999;
        assert_eq!(gsp_banded_retention(&s, &config), Some(0.95));
        s.stability = 20.001;
        assert_eq!(gsp_banded_retention(&s, &config), Some(0.80));
        // band=0：关闭 → None
        config.gsp_maturity_band_days = 0.0;
        assert_eq!(gsp_banded_retention(&s, &config), None);
    }

    /// round_half_to_even 对齐 Python banker's rounding（.5 取最近偶数）。
    #[test]
    fn gsp_round_half_to_even_matches_python() {
        assert_eq!(round_half_to_even(0.5), 0.0);
        assert_eq!(round_half_to_even(1.5), 2.0);
        assert_eq!(round_half_to_even(2.5), 2.0);
        assert_eq!(round_half_to_even(3.5), 4.0);
        assert_eq!(round_half_to_even(40.0), 40.0);
        assert_eq!(round_half_to_even(40.4), 40.0);
        assert_eq!(round_half_to_even(40.6), 41.0);
    }
}
