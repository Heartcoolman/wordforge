use proptest::prelude::*;

use learning_backend::amas::config::MemoryModelConfig;
use learning_backend::amas::memory::mdm::{
    compute_interval, recall_probability, update_strength, MdmState,
};

const DAY_MS: i64 = 86_400_000;

fn c23_config() -> impl Strategy<Value = MemoryModelConfig> {
    (
        prop::array::uniform19(0.01f64..20.0),
        0.10f64..0.35,
        (-0.9f64)..(-0.2),
        0.0f64..0.2,
    )
        .prop_map(|(mut w, factor, decay, floor)| {
            // Clamp w to C23 search bounds
            for i in 0..4 {
                w[i] = w[i].clamp(0.1, 20.0);
            }
            w[4] = w[4].clamp(3.0, 12.0);
            w[5] = w[5].clamp(0.1, 2.0);
            w[6] = w[6].clamp(0.3, 4.0);
            w[7] = w[7].clamp(0.001, 0.5);
            for i in 8..15 {
                let hi = if matches!(i, 9 | 12 | 13) { 1.0 } else { 4.0 };
                w[i] = w[i].clamp(0.01, hi);
            }
            w[15] = w[15].clamp(0.01, 1.0);
            w[16] = w[16].clamp(1.0, 5.0);
            w[17] = w[17].clamp(-1.0, 2.0);
            w[18] = w[18].clamp(-1.0, 2.0);

            let mut cfg = MemoryModelConfig::default();
            cfg.w = w;
            cfg.forgetting_curve_factor = factor;
            cfg.forgetting_curve_decay = decay;
            cfg.forgetting_curve_floor = floor;
            cfg
        })
}

fn quality_for_grade(grade: u32) -> f64 {
    match grade {
        1 => 0.0,
        2 => 0.3,
        3 => 0.7,
        4 => 0.95,
        _ => 0.7,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn pt_mdm_recall_bounded_and_monotonic(
        quality in 0.0_f64..1.0,
        alpha in 0.01_f64..0.99,
        delta1 in 1_i64..10_000,
        delta2 in 10_001_i64..20_000,
    ) {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        update_strength(&mut state, quality, alpha, chrono::Utc::now().timestamp_millis(), &config);
        let base = state.last_review_at.unwrap_or(0);

        let p1 = recall_probability(&state, base + delta1, &config);
        let p2 = recall_probability(&state, base + delta2, &config);

        prop_assert!((0.0..=1.0).contains(&p1));
        prop_assert!((0.0..=1.0).contains(&p2));
        prop_assert!(p2 <= p1);
    }

    #[test]
    fn pt_mdm_interval_positive(
        quality in 0.0_f64..1.0,
        target in 0.5_f64..0.99,
        scale in 0.1_f64..3.0,
    ) {
        let config = MemoryModelConfig::default();
        let mut state = MdmState::default();
        update_strength(&mut state, quality, 0.3, chrono::Utc::now().timestamp_millis(), &config);
        let interval = compute_interval(&state, target, scale, &config);
        prop_assert!(interval >= 0);
    }

    // --- Same-day review invariants ---

    #[test]
    fn pt_sameday_recursive_consistency(
        s0 in 0.5f64..50.0,
        grade in 1u32..=4,
        n in 1usize..10,
    ) {
        // 实现细节：mdm.rs::update_strength() 在 same-day 分支算出 target_stability = prev * k，
        // 然后通过 alpha 线性插值得到 new_stability = prev + alpha * (target - prev)
        //                                          = prev * (1 + alpha * (k - 1))。
        // 因此 n 次同日复习后期望值 = s0 * (1 + alpha*(k-1))^n（仍依赖 last_review_at 更新维持递推）。
        let config = MemoryModelConfig::default();
        let alpha: f64 = 0.3;
        let grade_f = grade as f64;
        let k = (config.w[17] * (grade_f - 3.0 + config.w[18])).clamp(-20.0, 20.0).exp();
        let step_factor = 1.0 + alpha * (k - 1.0);

        let now = 1_000_000_000_000i64;
        let mut state = MdmState::default();
        state.stability = s0;
        state.difficulty = 5.0;
        state.review_count = 1;
        state.last_review_at = Some(now);
        state.memory_strength = s0;

        for _ in 0..n {
            let review_at = state.last_review_at.unwrap() + 3_600_000; // 1 hour later
            update_strength(&mut state, quality_for_grade(grade), alpha, review_at, &config);
        }

        let expected = (s0 * step_factor.powi(n as i32)).max(0.01);
        let tolerance = expected.abs() * 1e-6 + 1e-9;
        prop_assert!(
            (state.stability - expected).abs() < tolerance,
            "S={} expected={} diff={}",
            state.stability, expected, (state.stability - expected).abs()
        );
    }

    #[test]
    fn pt_sameday_grade_monotonicity(
        s0 in 0.1f64..20.0,
    ) {
        let config = MemoryModelConfig::default();
        let now = 1_000_000_000_000i64;

        // Good (grade=3) should increase or maintain stability
        let mut state_good = MdmState::default();
        state_good.stability = s0;
        state_good.difficulty = 5.0;
        state_good.review_count = 1;
        state_good.last_review_at = Some(now);
        state_good.memory_strength = s0;
        update_strength(&mut state_good, 0.7, 0.3, now + 3_600_000, &config);

        // Again (grade=1) should decrease or maintain stability
        let mut state_again = MdmState::default();
        state_again.stability = s0;
        state_again.difficulty = 5.0;
        state_again.review_count = 1;
        state_again.last_review_at = Some(now);
        state_again.memory_strength = s0;
        update_strength(&mut state_again, 0.0, 0.3, now + 3_600_000, &config);

        if s0 > 0.01 {
            prop_assert!(state_good.stability >= s0, "Good same-day: {} >= {}", state_good.stability, s0);
            prop_assert!(state_again.stability <= s0, "Again same-day: {} <= {}", state_again.stability, s0);
        }
    }

    #[test]
    fn pt_sameday_boundary_strictness(
        s0 in 1.0f64..10.0,
        grade in 1u32..=4,
    ) {
        let config = MemoryModelConfig::default();
        let now = 1_000_000_000_000i64;

        // Just under 1 day: same-day path
        let mut state_short = MdmState::default();
        state_short.stability = s0;
        state_short.difficulty = 5.0;
        state_short.review_count = 1;
        state_short.last_review_at = Some(now);
        state_short.memory_strength = s0;
        update_strength(&mut state_short, quality_for_grade(grade), 0.3, now + DAY_MS - 1, &config);

        // Just over 1 day: long-term path
        let mut state_long = MdmState::default();
        state_long.stability = s0;
        state_long.difficulty = 5.0;
        state_long.review_count = 1;
        state_long.last_review_at = Some(now);
        state_long.memory_strength = s0;
        update_strength(&mut state_long, quality_for_grade(grade), 0.3, now + DAY_MS + 1, &config);

        // Exactly 1 day: long-term path
        let mut state_exact = MdmState::default();
        state_exact.stability = s0;
        state_exact.difficulty = 5.0;
        state_exact.review_count = 1;
        state_exact.last_review_at = Some(now);
        state_exact.memory_strength = s0;
        update_strength(&mut state_exact, quality_for_grade(grade), 0.3, now + DAY_MS, &config);

        // Short-term and long-term should differ
        prop_assert!(
            (state_short.stability - state_long.stability).abs() > 1e-12,
            "Boundary outputs must differ: short={} long={}",
            state_short.stability, state_long.stability
        );
        // Exact 1 day matches long-term (within tolerance due to different elapsed)
        let exact_long_diff = (state_exact.stability - state_long.stability).abs();
        prop_assert!(exact_long_diff < 0.01, "Exact-1-day vs just-over should be close: diff={}", exact_long_diff);
    }

    #[test]
    fn pt_stability_floor_preservation(
        s0 in 0.005f64..0.05,
        grade in 1u32..=4,
        same_day in proptest::bool::ANY,
    ) {
        let config = MemoryModelConfig::default();
        let now = 1_000_000_000_000i64;

        let mut state = MdmState::default();
        state.stability = s0;
        state.difficulty = 5.0;
        state.review_count = 1;
        state.last_review_at = Some(now);
        state.memory_strength = s0;

        let delta = if same_day { 3_600_000 } else { DAY_MS * 5 };
        update_strength(&mut state, quality_for_grade(grade), 0.3, now + delta, &config);
        prop_assert!(state.stability >= 0.01, "Floor violated: {}", state.stability);
    }

    #[test]
    fn pt_exponent_clamp_correctness(
        w17 in -100.0f64..100.0,
        w18 in -100.0f64..100.0,
        grade in 1u32..=4,
    ) {
        let grade_f = grade as f64;
        let clamped = (w17 * (grade_f - 3.0 + w18)).clamp(-20.0, 20.0);
        let result = clamped.exp();
        prop_assert!(result.is_finite(), "exp({}) = {} is not finite", clamped, result);
    }

    #[test]
    fn pt_sameday_commutativity(
        s0 in 1.0f64..20.0,
        g1 in 1u32..=4,
        g2 in 1u32..=4,
    ) {
        let config = MemoryModelConfig::default();
        let now = 1_000_000_000_000i64;

        // Order 1: g1 then g2
        let mut state_a = MdmState::default();
        state_a.stability = s0;
        state_a.difficulty = 5.0;
        state_a.review_count = 1;
        state_a.last_review_at = Some(now);
        state_a.memory_strength = s0;
        update_strength(&mut state_a, quality_for_grade(g1), 0.3, now + 1_800_000, &config);
        update_strength(&mut state_a, quality_for_grade(g2), 0.3, now + 3_600_000, &config);

        // Order 2: g2 then g1
        let mut state_b = MdmState::default();
        state_b.stability = s0;
        state_b.difficulty = 5.0;
        state_b.review_count = 1;
        state_b.last_review_at = Some(now);
        state_b.memory_strength = s0;
        update_strength(&mut state_b, quality_for_grade(g2), 0.3, now + 1_800_000, &config);
        update_strength(&mut state_b, quality_for_grade(g1), 0.3, now + 3_600_000, &config);

        // Stability should be order-independent when above floor
        if state_a.stability > 0.011 && state_b.stability > 0.011 {
            let tolerance = state_a.stability.abs() * 1e-6 + 1e-9;
            prop_assert!(
                (state_a.stability - state_b.stability).abs() < tolerance,
                "Commutativity: {} vs {} diff={}",
                state_a.stability, state_b.stability,
                (state_a.stability - state_b.stability).abs()
            );
        }
    }

    #[test]
    fn pt_cross_branch_safety(
        s0 in 0.5f64..20.0,
        steps in proptest::collection::vec((1u32..=4, proptest::bool::ANY), 2..8),
    ) {
        let config = MemoryModelConfig::default();
        let now = 1_000_000_000_000i64;

        let mut state = MdmState::default();
        state.stability = s0;
        state.difficulty = 5.0;
        state.review_count = 1;
        state.last_review_at = Some(now);
        state.memory_strength = s0;

        let mut current_time = now;
        for (grade, same_day) in &steps {
            let delta = if *same_day { 3_600_000 } else { DAY_MS * 2 };
            current_time += delta;
            update_strength(&mut state, quality_for_grade(*grade), 0.3, current_time, &config);
            prop_assert!(state.stability.is_finite(), "Non-finite stability after update");
            prop_assert!(state.stability >= 0.01, "Floor violated: {}", state.stability);
        }
    }

    #[test]
    fn pt_difficulty_invariance_across_branches(
        s0 in 1.0f64..10.0,
        d0 in 1.0f64..10.0,
        grade in 1u32..=4,
    ) {
        let config = MemoryModelConfig::default();
        let now = 1_000_000_000_000i64;

        // Same-day update
        let mut state_sd = MdmState::default();
        state_sd.stability = s0;
        state_sd.difficulty = d0;
        state_sd.review_count = 1;
        state_sd.last_review_at = Some(now);
        state_sd.memory_strength = s0;
        update_strength(&mut state_sd, quality_for_grade(grade), 0.3, now + 3_600_000, &config);

        // Long-term update
        let mut state_lt = MdmState::default();
        state_lt.stability = s0;
        state_lt.difficulty = d0;
        state_lt.review_count = 1;
        state_lt.last_review_at = Some(now);
        state_lt.memory_strength = s0;
        update_strength(&mut state_lt, quality_for_grade(grade), 0.3, now + DAY_MS * 2, &config);

        let tolerance = 1e-6;
        prop_assert!(
            (state_sd.difficulty - state_lt.difficulty).abs() < tolerance,
            "Difficulty should be same: sd={} lt={}",
            state_sd.difficulty, state_lt.difficulty
        );
    }

    #[test]
    fn pt_difficulty_always_in_range(
        w4 in 3.0f64..12.0,
        w5 in 0.1f64..2.0,
        w6 in 0.3f64..4.0,
        w7 in 0.001f64..0.5,
        grade in 1u32..=4,
        prior_d in 1.0f64..10.0,
    ) {
        let mut config = MemoryModelConfig::default();
        config.w[4] = w4;
        config.w[5] = w5;
        config.w[6] = w6;
        config.w[7] = w7;
        let now = 1_000_000_000_000i64;

        // First review
        let mut state = MdmState::default();
        update_strength(&mut state, quality_for_grade(grade), 0.3, now, &config);
        prop_assert!((1.0..=10.0).contains(&state.difficulty), "D0={}", state.difficulty);

        // Subsequent review
        state.difficulty = prior_d;
        update_strength(&mut state, quality_for_grade(grade), 0.3, now + DAY_MS * 2, &config);
        prop_assert!((1.0..=10.0).contains(&state.difficulty), "D'={}", state.difficulty);
    }

    #[test]
    fn pt_recall_monotonic_c23(
        config in c23_config(),
        quality in 0.0f64..1.0,
        d1 in 1i64..10_000,
        gap in 1i64..10_000,
    ) {
        let mut state = MdmState::default();
        let now = 1_000_000_000_000i64;
        update_strength(&mut state, quality, 0.3, now, &config);
        let base = state.last_review_at.unwrap();
        let d2 = d1 + gap;

        let p1 = recall_probability(&state, base + d1, &config);
        let p2 = recall_probability(&state, base + d2, &config);

        prop_assert!((0.0..=1.0).contains(&p1), "p1={}", p1);
        prop_assert!((0.0..=1.0).contains(&p2), "p2={}", p2);
        prop_assert!(p2 <= p1 + 1e-12, "Monotonicity: p2={} > p1={}", p2, p1);
    }

    #[test]
    fn pt_interval_range_c23(
        config in c23_config(),
        quality in 0.0f64..1.0,
        target in 0.5f64..0.99,
    ) {
        let mut state = MdmState::default();
        let now = 1_000_000_000_000i64;
        update_strength(&mut state, quality, 0.3, now, &config);
        let interval = compute_interval(&state, target, 1.0, &config);
        prop_assert!(interval >= 60, "interval={} < 60s", interval);
        prop_assert!(interval <= 7_776_000, "interval={} > 90 days", interval);
    }
}
