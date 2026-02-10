use proptest::prelude::*;

use learning_backend::amas::memory::mdm::{
    compute_interval, recall_probability, update_strength, MdmState,
};

proptest! {
    #[test]
    fn pt_mdm_recall_bounded_and_monotonic(
        quality in 0.0_f64..1.0,
        alpha in 0.01_f64..0.99,
        delta1 in 1_i64..10_000,
        delta2 in 10_001_i64..20_000,
    ) {
        let mut state = MdmState::default();
        update_strength(&mut state, quality, alpha);
        let base = state.last_review_at.unwrap_or(0);

        let p1 = recall_probability(&state, base + delta1);
        let p2 = recall_probability(&state, base + delta2);

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
        let mut state = MdmState::default();
        update_strength(&mut state, quality, 0.3);
        let interval = compute_interval(&state, target, scale);
        prop_assert!(interval >= 0);
    }
}
