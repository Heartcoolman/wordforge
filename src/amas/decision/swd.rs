use serde::{Deserialize, Serialize};

use crate::amas::config::AMASConfig;
use crate::amas::types::*;

const DECAY_HALF_LIFE_DAYS: f64 = 7.0;
const LN2: f64 = std::f64::consts::LN_2;
const CONFIDENCE_MIN: f64 = 0.2;
const CONFIDENCE_MAX: f64 = 0.9;
const NORMALIZATION_REF: f64 = 1_000_000.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwdState {
    pub strategy_history: Vec<StrategyRewardEntry>,
    pub max_history_size: usize,
}

impl Default for SwdState {
    fn default() -> Self {
        let config = crate::amas::config::SwdConfig::default();
        Self {
            strategy_history: Vec::new(),
            max_history_size: config.max_history_size,
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
    config: &AMASConfig,
) -> DecisionCandidate {
    let swd = &config.swd;

    if swd_state.strategy_history.is_empty() {
        return fallback_candidate(swd.fallback_confidence);
    }

    let mut difficulty_sum = 0.0;
    let mut batch_size_sum: f64 = 0.0;
    let mut new_ratio_sum = 0.0;
    let mut interval_scale_sum = 0.0;
    let mut total_weight = 0.0;
    let mut review_votes_for = 0.0;
    let mut review_votes_against = 0.0;

    let now_ms = chrono::Utc::now().timestamp_millis();
    for entry in &swd_state.strategy_history {
        if entry.reward <= swd.history_filter_threshold {
            continue;
        }

        let sim = similarity(user_state, &entry.user_state_snapshot);
        // 时间衰减：旧记录降权，半衰期为 7 天
        let age_ms = (now_ms - entry.timestamp).max(0) as f64;
        let half_life_ms = DECAY_HALF_LIFE_DAYS * 24.0 * 3600.0 * 1000.0;
        let time_decay = (-age_ms * LN2 / half_life_ms).exp();
        let weight = sim * time_decay;
        total_weight += weight;

        difficulty_sum += entry.strategy.difficulty * weight;
        batch_size_sum += entry.strategy.batch_size as f64 * weight;
        new_ratio_sum += entry.strategy.new_ratio * weight;
        interval_scale_sum += entry.strategy.interval_scale * weight;

        if entry.strategy.review_mode {
            review_votes_for += weight;
        } else {
            review_votes_against += weight;
        }
    }

    if total_weight <= 0.0 {
        return fallback_candidate(swd.fallback_confidence);
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
        confidence: (total_weight / swd_state.strategy_history.len() as f64).clamp(CONFIDENCE_MIN, CONFIDENCE_MAX),
        explanation: "Similarity-weighted strategy".to_string(),
    }
}

pub fn update(
    swd_state: &mut SwdState,
    user_state: &UserState,
    strategy: &StrategyParams,
    reward: f64,
    config: &AMASConfig,
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

    let max_size = config.swd.max_history_size;
    if swd_state.strategy_history.len() > max_size {
        let remove_count = swd_state.strategy_history.len() - max_size;
        swd_state.strategy_history.drain(0..remove_count);
    }
}

fn similarity(current: &UserState, history: &UserStateSnapshot) -> f64 {
    // 对 total_event_count 的 ln_1p 值做归一化，使其与 [0,1] 范围内的其他维度可比
    let max_ln = NORMALIZATION_REF.ln_1p();
    let current_events_norm = (current.total_event_count as f64).ln_1p() / max_ln;
    let history_events_norm = (history.total_event_count as f64).ln_1p() / max_ln;
    let distance = ((current.attention - history.attention).powi(2)
        + (current.fatigue - history.fatigue).powi(2)
        + (current.motivation - history.motivation).powi(2)
        + (current_events_norm - history_events_norm).powi(2))
    .sqrt();
    1.0 / (1.0 + distance)
}

fn fallback_candidate(confidence: f64) -> DecisionCandidate {
    DecisionCandidate {
        algorithm_id: AlgorithmId::Swd,
        strategy: StrategyParams::default(),
        confidence,
        explanation: "SWD fallback".to_string(),
    }
}
