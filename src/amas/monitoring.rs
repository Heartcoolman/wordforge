use serde::{Deserialize, Serialize};

use crate::amas::config::AMASConfig;
use crate::amas::types::*;
use crate::store::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantViolation {
    pub field: String,
    pub value: f64,
    pub expected_range: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringEvent {
    pub id: String,
    pub user_id: String,
    pub session_id: String,
    pub event_type: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub latency_ms: i64,
    pub is_anomaly: bool,
    pub invariant_violations: Vec<InvariantViolation>,
    pub user_state: serde_json::Value,
    pub strategy: serde_json::Value,
    pub reward: serde_json::Value,
    pub cold_start_phase: Option<String>,
    pub selection_constraints_met: bool,
    pub reward_value: f64,
    #[serde(default)]
    pub config_version: String,
}

pub fn check_invariants(result: &ProcessResult) -> Vec<InvariantViolation> {
    let mut violations = Vec::new();

    check_range(
        &mut violations,
        "attention",
        result.state.attention,
        0.0,
        1.0,
    );
    check_range(&mut violations, "fatigue", result.state.fatigue, 0.0, 1.0);
    check_range(
        &mut violations,
        "confidence",
        result.state.confidence,
        0.0,
        1.0,
    );
    check_range(
        &mut violations,
        "motivation",
        result.state.motivation,
        -1.0,
        1.0,
    );

    check_range(
        &mut violations,
        "difficulty",
        result.strategy.difficulty,
        0.0,
        1.0,
    );
    check_range(
        &mut violations,
        "new_ratio",
        result.strategy.new_ratio,
        0.0,
        1.0,
    );

    if result.strategy.batch_size < 1 {
        violations.push(InvariantViolation {
            field: "batch_size".to_string(),
            value: result.strategy.batch_size as f64,
            expected_range: ">= 1".to_string(),
        });
    }

    violations
}

fn check_range(
    violations: &mut Vec<InvariantViolation>,
    field: &str,
    value: f64,
    min: f64,
    max: f64,
) {
    if value.is_nan() {
        violations.push(InvariantViolation {
            field: field.to_string(),
            value: f64::NAN,
            expected_range: format!("[{min}, {max}]"),
        });
        return;
    }
    if value < min || value > max {
        violations.push(InvariantViolation {
            field: field.to_string(),
            value,
            expected_range: format!("[{min}, {max}]"),
        });
    }
}

pub fn should_sample(
    is_anomaly: bool,
    cold_start_phase: &Option<ColdStartPhase>,
    sample_rate: f64,
) -> bool {
    if is_anomaly {
        return true;
    }
    if cold_start_phase.is_some() {
        return true;
    }
    rand::random::<f64>() < sample_rate
}

pub fn compute_config_hash(config: &AMASConfig) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let json = serde_json::to_string(config).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    json.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[allow(clippy::too_many_arguments)]
pub fn record_event(
    store: &Store,
    user_id: &str,
    session_id: &str,
    result: &ProcessResult,
    latency_ms: i64,
    config: &AMASConfig,
    pre_constraint_strategy: &StrategyParams,
    config_version: &str,
) {
    let violations = check_invariants(result);
    let is_anomaly = !violations.is_empty();

    if !should_sample(
        is_anomaly,
        &result.cold_start_phase,
        config.monitoring.sample_rate,
    ) {
        return;
    }

    let selection_constraints_met = result.strategy == *pre_constraint_strategy;

    let event = MonitoringEvent {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        session_id: session_id.to_string(),
        event_type: "process_event".to_string(),
        timestamp: chrono::Utc::now(),
        latency_ms,
        is_anomaly,
        invariant_violations: violations,
        user_state: serde_json::to_value(&result.state).unwrap_or_default(),
        strategy: serde_json::to_value(&result.strategy).unwrap_or_default(),
        reward: serde_json::to_value(&result.reward).unwrap_or_default(),
        cold_start_phase: result.cold_start_phase.as_ref().map(|p| format!("{p:?}")),
        selection_constraints_met,
        reward_value: result.reward.value,
        config_version: config_version.to_string(),
    };

    if is_anomaly {
        tracing::warn!(user_id, violations=?event.invariant_violations, "AMAS invariant violation");
    }

    if let Err(e) = store.insert_monitoring_event(&serde_json::to_value(event).unwrap_or_default())
    {
        tracing::error!(error=%e, "Failed to persist monitoring event");
    }
}

#[cfg(test)]
// 测试用 `let mut cfg = X::default(); cfg.field = v` 易读，本 mod 豁免 field_reassign。
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::amas::types::{Explanation, Reward, RewardComponents};

    fn make_result(
        state: UserState,
        strategy: StrategyParams,
        cold_start_phase: Option<ColdStartPhase>,
    ) -> ProcessResult {
        ProcessResult {
            session_id: "s1".into(),
            strategy,
            explanation: Explanation {
                primary_reason: "test".into(),
                factors: vec![],
            },
            state,
            word_mastery: None,
            reward: Reward {
                value: 0.5,
                components: RewardComponents {
                    accuracy_reward: 0.5,
                    speed_reward: 0.5,
                    fatigue_penalty: 0.0,
                    frustration_penalty: 0.0,
                    expected_forget_cost: 0.0,
                },
            },
            cold_start_phase,
        }
    }

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn check_invariants_passes_for_valid_result() {
        let result = make_result(UserState::default(), StrategyParams::default(), None);
        let v = check_invariants(&result);
        assert!(v.is_empty());
    }

    #[test]
    fn check_invariants_detects_out_of_range_and_nan() {
        let mut state = UserState::default();
        state.attention = 2.0;
        state.fatigue = f64::NAN;
        state.motivation = -2.0;
        let strategy = StrategyParams {
            difficulty: 1.5,
            new_ratio: -0.1,
            batch_size: 0,
            ..StrategyParams::default()
        };
        let result = make_result(state, strategy, None);
        let v = check_invariants(&result);
        // attention out of range, fatigue NaN, motivation out, difficulty out,
        // new_ratio out, batch_size 0 → 6 violations 至少
        let fields: Vec<&str> = v.iter().map(|x| x.field.as_str()).collect();
        for expected in [
            "attention",
            "fatigue",
            "motivation",
            "difficulty",
            "new_ratio",
            "batch_size",
        ] {
            assert!(fields.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn should_sample_always_true_for_anomaly_or_cold_start() {
        assert!(should_sample(true, &None, 0.0));
        assert!(should_sample(false, &Some(ColdStartPhase::Classify), 0.0));
    }

    #[test]
    fn should_sample_false_at_zero_rate_normal() {
        assert!(!should_sample(false, &None, 0.0));
    }

    #[test]
    fn should_sample_true_at_full_rate_normal() {
        assert!(should_sample(false, &None, 1.0));
    }

    #[test]
    fn compute_config_hash_is_stable_and_diffs_on_change() {
        let mut c = AMASConfig::default();
        let h1 = compute_config_hash(&c);
        let h2 = compute_config_hash(&c);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
        c.monitoring.sample_rate = 0.99;
        let h3 = compute_config_hash(&c);
        assert_ne!(h1, h3);
    }

    #[test]
    fn record_event_persists_when_anomaly_present() {
        let (_t, store) = tempfile_store();
        let mut config = AMASConfig::default();
        // 强制不通过 sampling 的非 anomaly 路径
        config.monitoring.sample_rate = 0.0;
        let mut state = UserState::default();
        state.attention = 2.0; // 越界 → anomaly → 必落
        let strategy = StrategyParams::default();
        let result = make_result(state, strategy.clone(), None);
        record_event(&store, "u1", "s1", &result, 12, &config, &strategy, "v1");
        // 落库
        let evts = store.get_recent_monitoring_events(10).unwrap();
        assert!(!evts.is_empty());
    }

    #[test]
    fn record_event_skips_when_sampling_off_and_no_anomaly() {
        let (_t, store) = tempfile_store();
        let mut config = AMASConfig::default();
        config.monitoring.sample_rate = 0.0;
        let strategy = StrategyParams::default();
        let result = make_result(UserState::default(), strategy.clone(), None);
        record_event(&store, "u1", "s1", &result, 10, &config, &strategy, "v1");
        assert!(store.get_recent_monitoring_events(10).unwrap().is_empty());
    }

    #[test]
    fn record_event_logs_when_cold_start_phase_is_set() {
        let (_t, store) = tempfile_store();
        let mut config = AMASConfig::default();
        config.monitoring.sample_rate = 0.0;
        let strategy = StrategyParams::default();
        let result = make_result(
            UserState::default(),
            strategy.clone(),
            Some(ColdStartPhase::Explore),
        );
        record_event(&store, "u1", "s1", &result, 10, &config, &strategy, "v1");
        assert!(!store.get_recent_monitoring_events(10).unwrap().is_empty());
    }
}
