use proptest::prelude::*;

use learning_backend::amas::config::{AMASConfig, EnsembleConfig};
use learning_backend::amas::decision::{ensemble, heuristic};
use learning_backend::amas::types::{FeatureVector, UserState};

proptest! {
    #[test]
    fn pt_ens_weights_sum_to_one(total in 0_u64..1000) {
        let cfg = EnsembleConfig::default();
        let trust = ensemble::TrustScores::default();
        let weights = ensemble::get_weights(total, &trust, &cfg);
        let sum: f64 = weights.values().sum();
        prop_assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn pt_heuristic_output_in_valid_ranges(
        attention in 0.0_f64..1.0,
        fatigue in 0.0_f64..1.0,
        motivation in -1.0_f64..1.0,
        accuracy in 0.0_f64..1.0,
        speed in 0.0_f64..1.0,
        events in 0_u64..200,
    ) {
        let state = UserState {
            attention,
            fatigue,
            motivation,
            total_event_count: events,
            ..UserState::default()
        };
        let feature = FeatureVector {
            accuracy,
            response_speed: speed,
            quality: (accuracy * 0.6 + speed * 0.4).clamp(0.0, 1.0),
            engagement: 0.8,
            hint_penalty: 0.0,
            time_since_last_event_secs: 0.0,
            session_event_count: 0,
            is_quit: false,
        };

        let cfg = AMASConfig::default();
        let candidate = heuristic::generate(&state, &feature, &cfg);

        prop_assert!((0.0..=1.0).contains(&candidate.strategy.difficulty));
        prop_assert!((0.0..=1.0).contains(&candidate.strategy.new_ratio));
        prop_assert!(candidate.strategy.batch_size >= 1);
        prop_assert!(candidate.strategy.interval_scale > 0.0);
    }
}
