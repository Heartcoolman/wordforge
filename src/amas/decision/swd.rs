use serde::{Deserialize, Serialize};

use crate::amas::config::AMASConfig;
use crate::amas::types::*;

const DECAY_HALF_LIFE_DAYS: f64 = 7.0;
const LN2: f64 = std::f64::consts::LN_2;
const CONFIDENCE_MIN: f64 = 0.2;
const CONFIDENCE_MAX: f64 = 0.9;
const NORMALIZATION_REF: f64 = 1_000_000.0;
const NEGATIVE_EXPERIENCE_WEIGHT: f64 = 0.3;

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

    let now_ms = chrono::Utc::now().timestamp_millis();
    let similarities: Vec<f64> = swd_state
        .strategy_history
        .iter()
        .map(|e| similarity(user_state, &e.user_state_snapshot))
        .collect();

    let mut difficulty_sum = 0.0;
    let mut batch_size_sum: f64 = 0.0;
    let mut new_ratio_sum = 0.0;
    let mut interval_scale_sum = 0.0;
    let mut total_weight = 0.0;
    let mut review_votes_for = 0.0;
    let mut review_votes_against = 0.0;

    for (i, entry) in swd_state.strategy_history.iter().enumerate() {
        let sim = similarities[i];
        let age_ms = (now_ms - entry.timestamp).max(0) as f64;
        let half_life_ms = DECAY_HALF_LIFE_DAYS * 24.0 * 3600.0 * 1000.0;
        let time_decay = (-age_ms * LN2 / half_life_ms).exp();
        let mut weight = sim * time_decay;

        if entry.reward <= swd.history_filter_threshold {
            weight *= NEGATIVE_EXPERIENCE_WEIGHT;
        }

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
        confidence: (total_weight / swd_state.strategy_history.len() as f64)
            .clamp(CONFIDENCE_MIN, CONFIDENCE_MAX),
        explanation: "Similarity-weighted strategy".to_string(),
    }
}

/// push 本次 entry + 裁剪超额最旧，并**返回本次新增的 entry 克隆**。
/// 写放大重构：swd 历史已落追加式行表 `engine_swd_history`，引擎不再全量重写整个 Vec，而是把这条
/// 返回的 entry 交由 `persist_engine_state_atomic` 在同一原子 tx 内 append 一行。内存 Vec 的 push/裁剪
/// 仅服务本事件内（实际无后续读取），保持与旧 `update` 行为一致、便于纯内存单测继续守护 bit-exact。
pub fn update_returning(
    swd_state: &mut SwdState,
    user_state: &UserState,
    strategy: &StrategyParams,
    reward: f64,
    config: &AMASConfig,
) -> StrategyRewardEntry {
    let entry = StrategyRewardEntry {
        user_state_snapshot: UserStateSnapshot {
            attention: user_state.attention,
            fatigue: user_state.fatigue,
            motivation: user_state.motivation,
            total_event_count: user_state.total_event_count,
        },
        strategy: strategy.clone(),
        reward,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };
    swd_state.strategy_history.push(entry.clone());

    let max_size = config.swd.max_history_size;
    if swd_state.strategy_history.len() > max_size {
        let remove_count = swd_state.strategy_history.len() - max_size;
        swd_state.strategy_history.drain(0..remove_count);
    }
    entry
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_user_state() -> UserState {
        UserState {
            attention: 0.7,
            fatigue: 0.2,
            motivation: 0.4,
            total_event_count: 42,
            ..UserState::default()
        }
    }

    fn entry(
        snapshot: UserStateSnapshot,
        strategy: StrategyParams,
        reward: f64,
        timestamp: i64,
    ) -> StrategyRewardEntry {
        StrategyRewardEntry {
            user_state_snapshot: snapshot,
            strategy,
            reward,
            timestamp,
        }
    }

    fn expected_strategy(
        current: &UserState,
        state: &SwdState,
        config: &AMASConfig,
        now_ms: i64,
    ) -> StrategyParams {
        let mut difficulty_sum = 0.0;
        let mut batch_size_sum = 0.0;
        let mut new_ratio_sum = 0.0;
        let mut interval_scale_sum = 0.0;
        let mut total_weight = 0.0;
        let mut review_votes_for = 0.0;
        let mut review_votes_against = 0.0;

        for item in &state.strategy_history {
            let sim = similarity(current, &item.user_state_snapshot);
            let age_ms = (now_ms - item.timestamp).max(0) as f64;
            let half_life_ms = DECAY_HALF_LIFE_DAYS * 24.0 * 3600.0 * 1000.0;
            let time_decay = (-age_ms * LN2 / half_life_ms).exp();
            let mut weight = sim * time_decay;

            if item.reward <= config.swd.history_filter_threshold {
                weight *= NEGATIVE_EXPERIENCE_WEIGHT;
            }

            total_weight += weight;
            difficulty_sum += item.strategy.difficulty * weight;
            batch_size_sum += item.strategy.batch_size as f64 * weight;
            new_ratio_sum += item.strategy.new_ratio * weight;
            interval_scale_sum += item.strategy.interval_scale * weight;

            if item.strategy.review_mode {
                review_votes_for += weight;
            } else {
                review_votes_against += weight;
            }
        }

        StrategyParams {
            difficulty: (difficulty_sum / total_weight).clamp(0.0, 1.0),
            batch_size: (batch_size_sum / total_weight).round().max(1.0) as u32,
            new_ratio: (new_ratio_sum / total_weight).clamp(0.0, 1.0),
            interval_scale: (interval_scale_sum / total_weight).max(0.1),
            review_mode: review_votes_for > review_votes_against,
        }
    }

    fn assert_strategy_close(actual: &StrategyParams, expected: &StrategyParams) {
        assert!((actual.difficulty - expected.difficulty).abs() < 1e-9);
        assert_eq!(actual.batch_size, expected.batch_size);
        assert!((actual.new_ratio - expected.new_ratio).abs() < 1e-9);
        assert!((actual.interval_scale - expected.interval_scale).abs() < 1e-9);
        assert_eq!(actual.review_mode, expected.review_mode);
    }

    #[test]
    fn generate_uses_actual_history_even_when_lengths_match() {
        let config = AMASConfig::default();
        let current = sample_user_state();
        let now_ms = chrono::Utc::now().timestamp_millis();

        let state_a = SwdState {
            strategy_history: vec![
                entry(
                    UserStateSnapshot {
                        attention: 0.7,
                        fatigue: 0.2,
                        motivation: 0.4,
                        total_event_count: 42,
                    },
                    StrategyParams {
                        difficulty: 0.2,
                        batch_size: 6,
                        new_ratio: 0.2,
                        interval_scale: 0.8,
                        review_mode: false,
                    },
                    0.8,
                    now_ms,
                ),
                entry(
                    UserStateSnapshot {
                        attention: 0.2,
                        fatigue: 0.8,
                        motivation: -0.2,
                        total_event_count: 400,
                    },
                    StrategyParams {
                        difficulty: 0.9,
                        batch_size: 16,
                        new_ratio: 0.8,
                        interval_scale: 1.6,
                        review_mode: true,
                    },
                    0.8,
                    now_ms,
                ),
            ],
            ..SwdState::default()
        };
        let state_b = SwdState {
            strategy_history: vec![
                entry(
                    UserStateSnapshot {
                        attention: 0.2,
                        fatigue: 0.8,
                        motivation: -0.2,
                        total_event_count: 400,
                    },
                    StrategyParams {
                        difficulty: 0.15,
                        batch_size: 5,
                        new_ratio: 0.1,
                        interval_scale: 0.7,
                        review_mode: false,
                    },
                    0.8,
                    now_ms,
                ),
                entry(
                    UserStateSnapshot {
                        attention: 0.7,
                        fatigue: 0.2,
                        motivation: 0.4,
                        total_event_count: 42,
                    },
                    StrategyParams {
                        difficulty: 0.85,
                        batch_size: 18,
                        new_ratio: 0.9,
                        interval_scale: 1.7,
                        review_mode: true,
                    },
                    0.8,
                    now_ms,
                ),
            ],
            ..SwdState::default()
        };

        let expected_a = expected_strategy(&current, &state_a, &config, now_ms);
        let expected_b = expected_strategy(&current, &state_b, &config, now_ms);
        let actual_a = generate(&current, &state_a, &config).strategy;
        let actual_b = generate(&current, &state_b, &config).strategy;

        assert_strategy_close(&actual_a, &expected_a);
        assert_strategy_close(&actual_b, &expected_b);
        assert_ne!(actual_a, actual_b);
    }
}
