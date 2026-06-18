use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::amas::config::AMASConfig;
use crate::amas::types::*;
use crate::store::operations::amas_telemetry::VersionMetricsSlice;
use crate::store::Store;

/// canary 退化守卫的 baseline 输入（灰度起始时 stable 切片快照）。
/// 字段名兼容 `VersionMetricsSlice` 的 camelCase 序列化，可直接由 `baseline_metrics_json` 反序列化。
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanaryBaseline {
    /// baseline 切片自身样本量。退化 baseline（解析失败 default / 零样本 stable 切片）下
    /// event_count 不足，reward 轴对照不可信，须跳过以免漏报回滚。
    #[serde(default)]
    pub event_count: u64,
    #[serde(default)]
    pub mean_reward: f64,
    #[serde(default)]
    pub anomaly_rate: f64,
}

/// canary 退化守卫的最少样本量：live 切片 event_count 不足时跳过判定（避免早期噪声误回滚）。
pub const CANARY_MIN_SAMPLE: u64 = 50;

/// 纯判定：给定 baseline、live 切片与两阈值，是否应回滚。样本不足返回 false。
/// canary_monitor worker 与 promote_canary 复核共用此函数，确保「应回滚的退化 patch」在两条路径上判定一致。
pub fn should_rollback(
    baseline: &CanaryBaseline,
    live: &VersionMetricsSlice,
    reward_drop_threshold: f64,
    anomaly_rise_threshold: f64,
) -> bool {
    if live.event_count < CANARY_MIN_SAMPLE {
        return false;
    }
    // anomaly 轴：不依赖 baseline 样本量。baseline.anomaly_rate 退化为 0 时 anomaly_rise=live，
    // 仍能正确捕获异常率飙升，故始终生效（也是退化 baseline 下的兜底回滚轴）。
    let anomaly_rise = live.anomaly_rate - baseline.anomaly_rate;
    if anomaly_rise > anomaly_rise_threshold {
        return true;
    }
    // reward 轴：reward_drop = baseline - live，仅在 baseline 样本充足时可信。退化 baseline
    //（解析失败 default 或零样本 stable 切片，event_count < MIN_SAMPLE 且 mean_reward 记 0）下，
    // reward_drop 对任意正 live reward 恒为负 → reward 轴漏报回滚。故 baseline 样本不足时跳过
    // reward 轴，避免把"应回滚"误判为"通过"。
    if baseline.event_count < CANARY_MIN_SAMPLE {
        return false;
    }
    let reward_drop = baseline.mean_reward - live.mean_reward;
    reward_drop > reward_drop_threshold
}

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
    /// 本次决策最终采纳的主算法（权重最大者，与 algorithm_metrics_daily.algorithm_id 同口径小写）
    #[serde(default)]
    pub routing_algo: String,
    /// 各候选算法路由权重分布，键为算法小写名，值为权重（ensemble 多项、fallback 单项=1.0）
    #[serde(default)]
    pub routing_weights: serde_json::Value,
    /// 本次答题事件是否正确（来自 RawEvent.is_correct），用于按版本聚合命中率
    #[serde(default)]
    pub is_correct: bool,
    /// T1.3 A/B：实验切分维度（None=非实验事件）。independent of config_version，使 A/A 可分。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_arm: Option<String>,
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
    routing_weights: &HashMap<AlgorithmId, f64>,
    is_correct: bool,
    experiment: Option<(&str, &str)>,
) {
    let violations = check_invariants(result);
    let is_anomaly = !violations.is_empty();

    // T1.3:实验桶（experiment.is_some()）强制全采——canary percent 本就小，再叠 5% 采样会令
    // 引擎代理量率值方差极大、CI 过宽、should_rollback 易误判。非实验事件维持配置采样率。
    if experiment.is_none()
        && !should_sample(
            is_anomaly,
            &result.cold_start_phase,
            config.monitoring.sample_rate,
        )
    {
        return;
    }

    let selection_constraints_met = result.strategy == *pre_constraint_strategy;

    // 路由分布：主算法取权重最大者，权重 JSON 用小写算法名作键（对齐 algorithm_metrics_daily 口径）
    let routing_algo = routing_weights
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(algo, _)| algo.as_str().to_string())
        .unwrap_or_default();
    let routing_weights_value = serde_json::Value::Object(
        routing_weights
            .iter()
            .map(|(algo, w)| (algo.as_str().to_string(), serde_json::json!(w)))
            .collect(),
    );

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
        routing_algo,
        routing_weights: routing_weights_value,
        is_correct,
        experiment_id: experiment.map(|(id, _)| id.to_string()),
        experiment_arm: experiment.map(|(_, arm)| arm.to_string()),
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
        record_event(
            &store,
            "u1",
            "s1",
            &result,
            12,
            &config,
            &strategy,
            "v1",
            &HashMap::new(),
            false,
            None,
        );
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
        record_event(
            &store,
            "u1",
            "s1",
            &result,
            10,
            &config,
            &strategy,
            "v1",
            &HashMap::new(),
            false,
            None,
        );
        assert!(store.get_recent_monitoring_events(10).unwrap().is_empty());
    }

    fn slice(count: u64, reward: f64, anomaly: f64) -> VersionMetricsSlice {
        VersionMetricsSlice {
            version_hash: "h".into(),
            event_count: count,
            mean_reward: reward,
            anomaly_rate: anomaly,
            ..Default::default()
        }
    }

    #[test]
    fn should_rollback_on_reward_drop_or_anomaly_rise() {
        let baseline = CanaryBaseline {
            event_count: 100,
            mean_reward: 0.80,
            anomaly_rate: 0.01,
        };
        // reward 降 0.10 > 0.05
        assert!(should_rollback(&baseline, &slice(100, 0.70, 0.01), 0.05, 0.05));
        // anomaly 升 0.09 > 0.05
        assert!(should_rollback(&baseline, &slice(100, 0.80, 0.10), 0.05, 0.05));
        // 均在阈值内
        assert!(!should_rollback(&baseline, &slice(100, 0.78, 0.02), 0.05, 0.05));
    }

    #[test]
    fn should_rollback_skips_small_sample_and_degraded_baseline() {
        let baseline = CanaryBaseline {
            event_count: 100,
            mean_reward: 0.80,
            anomaly_rate: 0.01,
        };
        // live 样本 < 50 不判定
        assert!(!should_rollback(&baseline, &slice(10, 0.0, 1.0), 0.05, 0.05));
        // 退化 baseline（event_count=0）下 reward 轴跳过，但 anomaly 轴兜底
        let degraded = CanaryBaseline::default();
        assert!(!should_rollback(&degraded, &slice(100, 0.70, 0.02), 0.05, 0.05));
        assert!(should_rollback(&degraded, &slice(100, 0.70, 0.20), 0.05, 0.05));
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
        record_event(
            &store,
            "u1",
            "s1",
            &result,
            10,
            &config,
            &strategy,
            "v1",
            &HashMap::new(),
            false,
            None,
        );
        assert!(!store.get_recent_monitoring_events(10).unwrap().is_empty());
    }
}
