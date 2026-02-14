use crate::amas::config::AMASConfig;
use crate::amas::types::*;

const FATIGUE_DIFFICULTY_CAP: f64 = 0.4;
const FATIGUE_BATCH_SIZE_CAP: u32 = 5;
const FATIGUE_NEW_RATIO_CAP: f64 = 0.1;
const LOW_ACCURACY_DIFFICULTY_FLOOR: f64 = 0.1;
const LOW_MOTIVATION_DIFFICULTY_FLOOR: f64 = 0.2;

pub fn generate(
    user_state: &UserState,
    feature: &FeatureVector,
    config: &AMASConfig,
) -> DecisionCandidate {
    let h = &config.heuristic;
    let mut strategy = StrategyParams::default();

    if user_state.fatigue > config.constraints.high_fatigue_threshold {
        strategy.difficulty = strategy.difficulty.min(FATIGUE_DIFFICULTY_CAP);
        strategy.batch_size = strategy.batch_size.min(FATIGUE_BATCH_SIZE_CAP);
        strategy.new_ratio = strategy.new_ratio.min(FATIGUE_NEW_RATIO_CAP);
    }

    if user_state.attention < config.constraints.low_attention_threshold {
        strategy.review_mode = true;
        strategy.new_ratio = 0.0;
    }

    if feature.accuracy > 0.5 && feature.response_speed > 0.7 {
        strategy.difficulty = (strategy.difficulty + h.accuracy_speed_difficulty_boost).min(1.0);
    }

    if feature.accuracy < 0.5 {
        strategy.difficulty = (strategy.difficulty - h.low_accuracy_difficulty_drop).max(LOW_ACCURACY_DIFFICULTY_FLOOR);
        strategy.new_ratio = (strategy.new_ratio - h.low_accuracy_ratio_drop).max(0.0);
    }

    if user_state.motivation < config.constraints.low_motivation_threshold {
        strategy.difficulty = (strategy.difficulty - h.low_motivation_difficulty_drop).max(LOW_MOTIVATION_DIFFICULTY_FLOOR);
        strategy.batch_size = strategy.batch_size.min(h.low_motivation_max_batch);
    }

    // 冷启动覆盖：新用户事件不足时，使用保守的固定策略参数，
    // 覆盖前面所有规则的调整结果，确保初始体验稳定可控
    if user_state.total_event_count < h.cold_start_event_threshold {
        strategy.difficulty = h.cold_start_difficulty;
        strategy.batch_size = h.cold_start_batch_size;
        strategy.new_ratio = h.cold_start_new_ratio;
    }

    DecisionCandidate {
        algorithm_id: AlgorithmId::Heuristic,
        strategy,
        confidence: compute_confidence(user_state, h),
        explanation: "Rule-based strategy".to_string(),
    }
}

fn compute_confidence(state: &UserState, h: &crate::amas::config::HeuristicConfig) -> f64 {
    let decay =
        (state.total_event_count as f64 / h.confidence_decay_scale).min(h.confidence_decay_cap);
    (h.confidence_base - decay).max(h.confidence_min)
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
