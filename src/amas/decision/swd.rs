use serde::{Deserialize, Serialize};

use crate::amas::config::AMASConfig;
use crate::amas::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwdState {
    pub strategy_history: Vec<StrategyRewardEntry>,
    pub max_history_size: usize,
}

impl Default for SwdState {
    fn default() -> Self {
        Self {
            strategy_history: Vec::new(),
            max_history_size: 200,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRewardEntry {
    pub user_state_snapshot: UserStateSnapshot,
    pub strategy: StrategyParams,
    pub reward: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStateSnapshot {
    pub attention: f64,
    pub fatigue: f64,
    pub motivation: f64,
    pub total_event_count: u64,
}

pub fn generate(
    user_state: &UserState,
    swd_state: &SwdState,
    _config: &AMASConfig,
) -> DecisionCandidate {
    if swd_state.strategy_history.is_empty() {
        return fallback_candidate();
    }

    let mut difficulty_sum = 0.0;
    let mut batch_size_sum: f64 = 0.0;
    let mut new_ratio_sum = 0.0;
    let mut interval_scale_sum = 0.0;
    let mut total_weight = 0.0;
    let mut review_votes_for = 0.0;
    let mut review_votes_against = 0.0;

    for entry in &swd_state.strategy_history {
        if entry.reward <= -0.5 {
            continue;
        }

        let sim = similarity(user_state, &entry.user_state_snapshot);
        total_weight += sim;

        difficulty_sum += entry.strategy.difficulty * sim;
        batch_size_sum += entry.strategy.batch_size as f64 * sim;
        new_ratio_sum += entry.strategy.new_ratio * sim;
        interval_scale_sum += entry.strategy.interval_scale * sim;

        if entry.strategy.review_mode {
            review_votes_for += sim;
        } else {
            review_votes_against += sim;
        }
    }

    if total_weight <= 0.0 {
        return fallback_candidate();
    }

    let strategy = StrategyParams {
        difficulty: (difficulty_sum / total_weight).clamp(0.0, 1.0),
        batch_size: (batch_size_sum / total_weight).round().max(1.0) as u32,
        new_ratio: (new_ratio_sum / total_weight).clamp(0.0, 1.0),
        interval_scale: (interval_scale_sum / total_weight).max(0.1),
        review_mode: review_votes_for > review_votes_against,
    };

    DecisionCandidate {
        algorithm_id: AlgorithmId::Swd,
        strategy,
        confidence: (total_weight / swd_state.strategy_history.len() as f64).clamp(0.2, 0.9),
        explanation: "Similarity-weighted strategy".to_string(),
    }
}

pub fn update(
    swd_state: &mut SwdState,
    user_state: &UserState,
    strategy: &StrategyParams,
    reward: f64,
) {
    swd_state.strategy_history.push(StrategyRewardEntry {
        user_state_snapshot: UserStateSnapshot {
            attention: user_state.attention,
            fatigue: user_state.fatigue,
            motivation: user_state.motivation,
            total_event_count: user_state.total_event_count,
        },
        strategy: strategy.clone(),
        reward,
        timestamp: chrono::Utc::now().timestamp_millis(),
    });

    if swd_state.strategy_history.len() > swd_state.max_history_size {
        let remove_count = swd_state.strategy_history.len() - swd_state.max_history_size;
        swd_state.strategy_history.drain(0..remove_count);
    }
}

fn similarity(current: &UserState, history: &UserStateSnapshot) -> f64 {
    let distance = ((current.attention - history.attention).powi(2)
        + (current.fatigue - history.fatigue).powi(2)
        + (current.motivation - history.motivation).powi(2)
        + ((current.total_event_count as f64).ln_1p()
            - (history.total_event_count as f64).ln_1p())
        .powi(2))
    .sqrt();
    1.0 / (1.0 + distance)
}

fn fallback_candidate() -> DecisionCandidate {
    DecisionCandidate {
        algorithm_id: AlgorithmId::Swd,
        strategy: StrategyParams::default(),
        confidence: 0.2,
        explanation: "SWD fallback".to_string(),
    }
}
