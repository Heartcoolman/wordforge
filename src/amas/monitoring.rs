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

pub fn record_event(
    store: &Store,
    user_id: &str,
    session_id: &str,
    result: &ProcessResult,
    latency_ms: i64,
    config: &AMASConfig,
    pre_constraint_strategy: &StrategyParams,
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
    };

    if is_anomaly {
        tracing::warn!(user_id, violations=?event.invariant_violations, "AMAS invariant violation");
    }

    if let Err(e) = store.insert_monitoring_event(&serde_json::to_value(event).unwrap_or_default())
    {
        tracing::error!(error=%e, "Failed to persist monitoring event");
    }
}
