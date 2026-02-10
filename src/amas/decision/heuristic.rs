use crate::amas::config::AMASConfig;
use crate::amas::types::*;

pub fn generate(
    user_state: &UserState,
    feature: &FeatureVector,
    config: &AMASConfig,
) -> DecisionCandidate {
    let mut strategy = StrategyParams::default();

    if user_state.fatigue > config.constraints.high_fatigue_threshold {
        strategy.difficulty = strategy.difficulty.min(0.4);
        strategy.batch_size = strategy.batch_size.min(5);
        strategy.new_ratio = strategy.new_ratio.min(0.1);
    }

    if user_state.attention < config.constraints.low_attention_threshold {
        strategy.review_mode = true;
        strategy.new_ratio = 0.0;
    }

    if feature.accuracy > 0.5 && feature.response_speed > 0.7 {
        strategy.difficulty = (strategy.difficulty + 0.1).min(1.0);
    }

    if feature.accuracy < 0.5 {
        strategy.difficulty = (strategy.difficulty - 0.15).max(0.1);
        strategy.new_ratio = (strategy.new_ratio - 0.1).max(0.0);
    }

    if user_state.motivation < config.constraints.low_motivation_threshold {
        strategy.difficulty = (strategy.difficulty - 0.1).max(0.2);
        strategy.batch_size = strategy.batch_size.min(8);
    }

    if user_state.total_event_count < 10 {
        strategy.difficulty = 0.3;
        strategy.batch_size = 5;
        strategy.new_ratio = 0.5;
    }

    DecisionCandidate {
        algorithm_id: AlgorithmId::Heuristic,
        strategy,
        confidence: compute_confidence(user_state),
        explanation: "Rule-based strategy".to_string(),
    }
}

fn compute_confidence(state: &UserState) -> f64 {
    let base = 0.7;
    let decay = (state.total_event_count as f64 / 200.0).min(0.5);
    (base - decay).max(0.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_fatigue_lowers_difficulty() {
        let mut state = UserState::default();
        state.fatigue = 0.95;
        let feature = FeatureVector {
            accuracy: 1.0,
            response_speed: 0.9,
            quality: 0.9,
            engagement: 0.8,
            hint_penalty: 0.0,
            time_since_last_event_secs: 0.0,
            session_event_count: 1,
            is_quit: false,
        };

        let c = generate(&state, &feature, &AMASConfig::default());
        assert!(c.strategy.difficulty <= 0.4);
        assert!(c.strategy.batch_size <= 5);
    }
}
