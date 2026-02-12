use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::amas::config::AMASConfig;
use crate::amas::decision::{ensemble, heuristic, ige, swd};
use crate::amas::memory::{iad, mastery, mdm, mtp};
use crate::amas::metrics;
use crate::amas::monitoring;
use crate::amas::types::*;
use crate::response::AppError;
use crate::store::Store;

const USER_LOCK_CLEANUP_THRESHOLD: usize = 500;
const SIGNAL_THRESHOLD: f64 = 0.5;
const TREND_BASELINE: f64 = 0.5;

/// 清理浮点数，将 NaN 和 Infinity 替换为安全默认值
fn sanitize_float(value: f64, default: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        default
    }
}

pub struct AMASEngine {
    config: Arc<RwLock<AMASConfig>>,
    store: Arc<Store>,
    user_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    metrics_registry: Arc<metrics::MetricsRegistry>,
}

#[derive(Debug, Clone, Default)]
pub struct AlgoStates {
    pub ige: ige::IgeState,
    pub swd: swd::SwdState,
    pub trust_scores: ensemble::TrustScores,
}

impl AMASEngine {
    pub fn new(config: AMASConfig, store: Arc<Store>) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            store,
            user_locks: Arc::new(Mutex::new(HashMap::new())),
            metrics_registry: Arc::new(metrics::MetricsRegistry::new()),
        }
    }

    pub async fn reload_config(&self, new_config: AMASConfig) -> Result<(), String> {
        new_config.validate()?;
        let mut cfg = self.config.write().await;
        *cfg = new_config;
        tracing::info!("AMAS config reloaded");
        Ok(())
    }

    pub async fn get_config(&self) -> AMASConfig {
        self.config.read().await.clone()
    }

    pub fn metrics_registry(&self) -> &Arc<metrics::MetricsRegistry> {
        &self.metrics_registry
    }

    async fn acquire_user_lock(&self, user_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.user_locks.lock().await;

        // 定期清理不再持有的用户锁。
        // Arc::strong_count == 1 表示只有 HashMap 持有引用，锁处于空闲状态，可安全移除。
        if locks.len() > USER_LOCK_CLEANUP_THRESHOLD {
            let before = locks.len();
            locks.retain(|_, v| Arc::strong_count(v) > 1);
            let removed = before - locks.len();
            if removed > 0 {
                tracing::info!(
                    before_count = before,
                    after_count = locks.len(),
                    removed_count = removed,
                    "清理空闲用户锁"
                );
            }
        }

        locks
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn process_event(
        &self,
        user_id: &str,
        raw_event: RawEvent,
    ) -> Result<ProcessResult, AppError> {
        let start = std::time::Instant::now();

        let user_lock = self.acquire_user_lock(user_id).await;
        let _guard = user_lock.lock().await;

        let config = self.config.read().await.clone();

        let mut user_state = self.load_or_init_state(user_id)?;
        let mut algo_states = self.load_algo_states(user_id)?;

        let feature = self.build_feature_vector(&raw_event, &user_state, &config);
        self.update_modeling(&mut user_state, &feature, &config);

        let cold_start_phase = self.determine_cold_start_phase(&user_state, &config);

        let candidates = self.generate_candidates(&user_state, &feature, &mut algo_states, &config);
        let (final_strategy, weights) =
            self.ensemble_or_fallback(&candidates, &user_state, &algo_states, &config);

        let reward = self.compute_reward(&feature, &user_state, &config);
        let word_mastery =
            self.update_memory(user_id, &raw_event, &feature, &final_strategy, &user_state, &config)?;

        let retention_signal = word_mastery
            .as_ref()
            .map(|wm| wm.recall_probability)
            .unwrap_or(0.0);
        let _objective = self.evaluate_objective(&reward, retention_signal, &config);

        let constrained_strategy =
            self.apply_constraints(final_strategy.clone(), &user_state, &config);

        self.update_trust_scores(
            &mut algo_states,
            &candidates,
            reward.value,
            &user_state,
            &weights,
            &config,
        );

        user_state.session_event_count += 1;
        user_state.total_event_count += 1;
        user_state.last_active_at = Some(chrono::Utc::now());

        // 检测 session 切换，重置 session 事件计数
        let current_session_id = raw_event
            .session_id
            .as_deref()
            .unwrap_or("");
        if !current_session_id.is_empty() {
            let session_changed = user_state
                .last_session_id
                .as_deref()
                .map_or(true, |prev| prev != current_session_id);
            if session_changed {
                user_state.session_event_count = 1;
                user_state.last_session_id = Some(current_session_id.to_string());
            }
        }

        self.persist_state(user_id, &user_state, &algo_states)?;

        let explanation = self.build_explanation(&constrained_strategy, &user_state, &weights);

        let session_id = raw_event
            .session_id
            .clone()
            .unwrap_or_else(|| format!("{user_id}-session"));

        let result = ProcessResult {
            session_id: session_id.clone(),
            strategy: constrained_strategy,
            explanation,
            state: user_state.clone(),
            word_mastery,
            reward: reward.clone(),
            cold_start_phase,
        };

        let latency_ms = start.elapsed().as_millis() as i64;
        self.emit_monitoring(
            user_id,
            &session_id,
            &result,
            latency_ms,
            &config,
            &final_strategy,
        );

        Ok(result)
    }

    pub async fn update_visual_fatigue(
        &self,
        user_id: &str,
        visual_score: f64,
    ) -> Result<UserState, AppError> {
        let user_lock = self.acquire_user_lock(user_id).await;
        let _guard = user_lock.lock().await;

        let config = self.config.read().await.clone();
        let mut user_state = self.load_or_init_state(user_id)?;

        // 将 0-100 映射到 0.0-1.0
        let visual_fatigue = (visual_score / 100.0).clamp(0.0, 1.0);

        // 混合公式：behavioral_weight * 行为疲劳 + visual_weight * 视觉疲劳
        let w = config.modeling.visual_fatigue_weight;
        user_state.fatigue = ((1.0 - w) * user_state.fatigue + w * visual_fatigue).clamp(0.0, 1.0);

        // 持久化前清理浮点数值
        user_state.fatigue = sanitize_float(user_state.fatigue, 0.0).clamp(0.0, 1.0);

        // 持久化
        let user_state_json =
            serde_json::to_value(&user_state).map_err(|e| AppError::internal(&e.to_string()))?;
        self.store
            .set_engine_user_state(user_id, &user_state_json)
            .map_err(|e| AppError::internal(&e.to_string()))?;

        Ok(user_state)
    }

    pub fn get_user_state(&self, user_id: &str) -> Result<UserState, AppError> {
        self.load_or_init_state(user_id)
    }

    pub fn compute_strategy_from_state(&self, user_state: &UserState) -> StrategyParams {
        // 注意：使用 try_read 可能在写锁期间回退默认值。
        // 对于精确结果，调用方应使用 compute_strategy_from_state_with_config 并传入已获取的 config。
        let config = self
            .config
            .try_read()
            .map(|c| c.clone())
            .unwrap_or_default();
        self.compute_strategy_from_state_with_config(user_state, &config)
    }

    pub fn compute_strategy_from_state_with_config(
        &self,
        user_state: &UserState,
        config: &AMASConfig,
    ) -> StrategyParams {
        let ls = &config.learning_strategy;
        let mut strategy = StrategyParams::default();

        // Adjust difficulty based on user confidence and motivation
        if user_state.confidence > ls.confidence_boost_threshold {
            strategy.difficulty = (strategy.difficulty + ls.confidence_difficulty_boost).min(1.0);
        }
        if user_state.motivation > ls.motivation_ratio_threshold {
            strategy.new_ratio = (strategy.new_ratio + ls.motivation_ratio_boost).min(1.0);
        }
        if user_state.fatigue > ls.fatigue_reduction_threshold {
            strategy.batch_size =
                (strategy.batch_size as f64 * ls.fatigue_batch_scale).max(3.0) as u32;
            strategy.difficulty = (strategy.difficulty - ls.fatigue_difficulty_drop).max(0.1);
        }

        strategy
    }

    pub async fn get_phase(&self, user_id: &str) -> Result<Option<ColdStartPhase>, AppError> {
        let state = self.load_or_init_state(user_id)?;
        let config = self.config.read().await.clone();
        Ok(self.determine_cold_start_phase(&state, &config))
    }

    pub fn reset_user_state(&self, user_id: &str) -> Result<(), AppError> {
        self.store
            .set_engine_user_state(
                user_id,
                &serde_json::to_value(&UserState::default())
                    .map_err(|e| AppError::internal(&e.to_string()))?,
            )
            .map_err(|e| AppError::internal(&e.to_string()))?;

        // 通过 Store 封装方法清除算法状态
        for algo in &["ige", "swd", "trust"] {
            self.store
                .delete_engine_algo_state(user_id, algo)
                .map_err(|e| AppError::internal(&e.to_string()))?;
        }

        Ok(())
    }

    pub async fn update_temporal_profile(
        &self,
        user_id: &str,
        hour: u8,
        accuracy: f64,
        avg_response_time_ms: f64,
        mastery_efficiency: f64,
    ) -> Result<(), AppError> {
        let user_lock = self.acquire_user_lock(user_id).await;
        let _guard = user_lock.lock().await;

        let config = self.config.read().await.clone();
        let mut user_state = self.load_or_init_state(user_id)?;
        let stats = &mut user_state.habit_profile.temporal_performance;
        let idx = (hour as usize).min(23);
        let h = &mut stats.hourly_stats[idx];

        // EMA 指数平滑
        let alpha = config.feature.temporal_profile_alpha;
        if h.session_count == 0 {
            h.avg_accuracy = accuracy;
            h.avg_response_time_ms = avg_response_time_ms;
            h.mastery_efficiency = mastery_efficiency;
        } else {
            h.avg_accuracy = h.avg_accuracy * (1.0 - alpha) + accuracy * alpha;
            h.avg_response_time_ms =
                h.avg_response_time_ms * (1.0 - alpha) + avg_response_time_ms * alpha;
            h.mastery_efficiency =
                h.mastery_efficiency * (1.0 - alpha) + mastery_efficiency * alpha;
        }
        h.session_count += 1;
        stats.total_sessions += 1;

        // 持久化
        let user_state_json =
            serde_json::to_value(&user_state).map_err(|e| AppError::internal(&e.to_string()))?;
        self.store
            .set_engine_user_state(user_id, &user_state_json)
            .map_err(|e| AppError::internal(&e.to_string()))?;
        Ok(())
    }

    pub fn get_temporal_boost(&self, user_id: &str, hour: u8) -> Result<f64, AppError> {
        let config = self
            .config
            .try_read()
            .map(|c| c.clone())
            .unwrap_or_default();
        let state = self.load_or_init_state(user_id)?;
        let stats = &state.habit_profile.temporal_performance;
        let idx = (hour as usize).min(23);
        let h = &stats.hourly_stats[idx];

        if h.session_count == 0 {
            return Ok(1.0);
        }

        let f = &config.feature;
        let boost = f.temporal_boost_base + h.mastery_efficiency * f.temporal_boost_scale;
        Ok(boost.clamp(f.temporal_boost_min, f.temporal_boost_max))
    }

    fn load_or_init_state(&self, user_id: &str) -> Result<UserState, AppError> {
        match self
            .store
            .get_engine_user_state(user_id)
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            Some(json) => serde_json::from_value(json)
                .map_err(|e| AppError::internal(&format!("State deserialize: {e}"))),
            None => Ok(UserState::default()),
        }
    }

    fn load_algo_states(&self, user_id: &str) -> Result<AlgoStates, AppError> {
        let mut states = AlgoStates::default();

        if let Some(v) = self
            .store
            .get_engine_algo_state(user_id, "ige")
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            states.ige = match serde_json::from_value(v) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(user_id, algo = "ige", error = %e, "Algo state deserialization failed, using default");
                    ige::IgeState::default()
                }
            };
        }

        if let Some(v) = self
            .store
            .get_engine_algo_state(user_id, "swd")
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            states.swd = match serde_json::from_value(v) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(user_id, algo = "swd", error = %e, "Algo state deserialization failed, using default");
                    swd::SwdState::default()
                }
            };
        }

        if let Some(v) = self
            .store
            .get_engine_algo_state(user_id, "trust")
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            states.trust_scores = match serde_json::from_value(v) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(user_id, algo = "trust", error = %e, "Algo state deserialization failed, using default");
                    ensemble::TrustScores::default()
                }
            };
        }

        Ok(states)
    }

    fn build_feature_vector(
        &self,
        event: &RawEvent,
        state: &UserState,
        config: &AMASConfig,
    ) -> FeatureVector {
        let m = &config.modeling;
        let accuracy = if event.is_correct { 1.0 } else { 0.0 };
        let response_speed = (1.0
            - (event.response_time_ms.max(0) as f64 / m.response_speed_max_ms))
            .clamp(0.0, 1.0);
        let f = &config.feature;
        let hint_penalty = if event.hint_used { f.hint_penalty } else { 0.0 };
        let quality = (accuracy * f.quality_accuracy_weight
            + response_speed * f.quality_speed_weight
            - hint_penalty)
            .clamp(0.0, 1.0);
        let engagement = Self::compute_engagement(event, m);

        let time_since_last = match state.last_active_at {
            Some(last) => (chrono::Utc::now() - last).num_seconds() as f64,
            None => 0.0,
        };

        FeatureVector {
            accuracy,
            response_speed,
            quality,
            engagement,
            hint_penalty,
            time_since_last_event_secs: time_since_last,
            session_event_count: state.session_event_count,
            is_quit: event.is_quit,
        }
    }

    fn compute_engagement(event: &RawEvent, m: &crate::amas::config::ModelingConfig) -> f64 {
        let mut score = 1.0;
        if let Some(pause) = event.pause_count {
            score -= (pause.max(0) as f64 * m.engagement_pause_penalty)
                .min(m.engagement_pause_penalty_max);
        }
        if let Some(switch) = event.switch_count {
            score -= (switch.max(0) as f64 * m.engagement_switch_penalty)
                .min(m.engagement_switch_penalty_max);
        }
        if let Some(focus_loss) = event.focus_loss_duration_ms {
            score -= (focus_loss.max(0) as f64 / m.engagement_focus_loss_base_ms)
                .min(m.engagement_focus_loss_penalty_max);
        }
        score.clamp(0.0, 1.0)
    }

    fn update_modeling(&self, state: &mut UserState, feature: &FeatureVector, config: &AMASConfig) {
        let m = &config.modeling;

        state.attention = state.attention * (1.0 - m.attention_smoothing)
            + feature.engagement * m.attention_smoothing;
        state.attention = state.attention.clamp(0.0, 1.0);

        // 先执行时间衰减，再增加新的疲劳值
        if feature.time_since_last_event_secs >= config.fatigue_decay.full_reset_threshold_secs {
            // >= full_reset_threshold: 完全重置
            state.fatigue = 0.0;
        } else if feature.time_since_last_event_secs
            > config.fatigue_decay.decay_start_threshold_secs
        {
            // decay_start ~ full_reset: 指数衰减（只对超过阈值的部分衰减）
            let elapsed_in_decay = feature.time_since_last_event_secs
                - config.fatigue_decay.decay_start_threshold_secs;
            let decay_factor =
                (-elapsed_in_decay / config.fatigue_decay.decay_time_constant_secs).exp();
            state.fatigue *= decay_factor;
        }

        if feature.is_quit {
            state.fatigue = (state.fatigue + m.fatigue_quit_increase).min(1.0);
        } else {
            state.fatigue = (state.fatigue + m.fatigue_increase_rate).min(1.0);
        }

        let motivation_signal = if feature.accuracy > SIGNAL_THRESHOLD {
            config.feature.motivation_positive_signal
        } else {
            config.feature.motivation_negative_signal
        };
        state.motivation = state.motivation * (1.0 - m.motivation_momentum)
            + motivation_signal * m.motivation_momentum;
        state.motivation = state.motivation.clamp(-1.0, 1.0);

        let confidence_signal = if feature.quality > SIGNAL_THRESHOLD {
            config.feature.confidence_positive_signal
        } else {
            config.feature.confidence_negative_signal
        };
        state.confidence = (state.confidence * m.confidence_decay + confidence_signal)
            .clamp(m.min_confidence, 1.0);

        // 更新认知画像
        let alpha = m.cognitive_profile_alpha;
        state.cognitive_profile.processing_speed = state.cognitive_profile.processing_speed
            * (1.0 - alpha)
            + feature.response_speed * alpha;
        state.cognitive_profile.memory_capacity =
            state.cognitive_profile.memory_capacity * (1.0 - alpha) + feature.accuracy * alpha;
        state.cognitive_profile.stability =
            state.cognitive_profile.stability * (1.0 - alpha) + feature.quality * alpha;

        // 更新趋势状态
        let trend_alpha = m.trend_alpha;
        state.trend_state.accuracy_trend = state.trend_state.accuracy_trend * (1.0 - trend_alpha)
            + (feature.accuracy - TREND_BASELINE) * trend_alpha;
        state.trend_state.speed_trend = state.trend_state.speed_trend * (1.0 - trend_alpha)
            + (feature.response_speed - TREND_BASELINE) * trend_alpha;
        state.trend_state.engagement_trend = state.trend_state.engagement_trend
            * (1.0 - trend_alpha)
            + (feature.engagement - TREND_BASELINE) * trend_alpha;
    }

    fn determine_cold_start_phase(
        &self,
        state: &UserState,
        config: &AMASConfig,
    ) -> Option<ColdStartPhase> {
        let cs = &config.cold_start;
        if state.total_event_count < cs.classify_to_explore_events {
            Some(ColdStartPhase::Classify)
        } else if state.total_event_count < cs.explore_to_exploit_events {
            // B28: Enhanced with AUC-based learner type classification
            Some(ColdStartPhase::Explore)
        } else {
            None
        }
    }

    /// B28: Classify learner type based on performance profile
    pub fn classify_learner_type(&self, user_id: &str) -> Result<LearnerType, AppError> {
        let config = self
            .config
            .try_read()
            .map(|c| c.clone())
            .unwrap_or_default();
        let state = self.load_or_init_state(user_id)?;
        let cp = &state.cognitive_profile;
        let cl = &config.classifier;

        let auc = cp.processing_speed * cl.processing_speed_weight
            + cp.memory_capacity * cl.memory_capacity_weight
            + cp.stability * cl.stability_weight;
        if auc > cl.fast_learner_threshold {
            Ok(LearnerType::Fast)
        } else if auc > cl.stable_learner_threshold {
            Ok(LearnerType::Stable)
        } else {
            Ok(LearnerType::Cautious)
        }
    }

    fn generate_candidates(
        &self,
        user_state: &UserState,
        feature: &FeatureVector,
        algo_states: &mut AlgoStates,
        config: &AMASConfig,
    ) -> Vec<DecisionCandidate> {
        let mut candidates = Vec::new();

        if config.feature_flags.heuristic_enabled {
            let start = std::time::Instant::now();
            candidates.push(heuristic::generate(user_state, feature, config));
            self.metrics_registry.record_call(
                AlgorithmId::Heuristic,
                start.elapsed().as_micros() as u64,
                false,
            );
        }

        if config.feature_flags.ige_enabled {
            let start = std::time::Instant::now();
            candidates.push(ige::generate(user_state, feature, &algo_states.ige, config));
            self.metrics_registry.record_call(
                AlgorithmId::Ige,
                start.elapsed().as_micros() as u64,
                false,
            );
        }

        if config.feature_flags.swd_enabled {
            let start = std::time::Instant::now();
            candidates.push(swd::generate(user_state, &algo_states.swd, config));
            self.metrics_registry.record_call(
                AlgorithmId::Swd,
                start.elapsed().as_micros() as u64,
                false,
            );
        }

        candidates
    }

    fn ensemble_or_fallback(
        &self,
        candidates: &[DecisionCandidate],
        user_state: &UserState,
        algo_states: &AlgoStates,
        config: &AMASConfig,
    ) -> (StrategyParams, HashMap<AlgorithmId, f64>) {
        if candidates.is_empty() {
            return (StrategyParams::default(), HashMap::new());
        }

        if config.feature_flags.ensemble_enabled && candidates.len() > 1 {
            let weights = ensemble::get_weights_for_candidates(
                candidates,
                user_state.total_event_count,
                &algo_states.trust_scores,
                &config.ensemble,
            );
            let strategy = ensemble::merge(candidates, &weights);
            return (strategy, weights);
        }

        let chosen = candidates
            .iter()
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        let mut weights = HashMap::new();
        weights.insert(chosen.algorithm_id, 1.0);
        (chosen.strategy.clone(), weights)
    }

    fn compute_reward(
        &self,
        feature: &FeatureVector,
        state: &UserState,
        config: &AMASConfig,
    ) -> Reward {
        let r = &config.reward;
        let accuracy_reward = feature.accuracy;
        let speed_reward = feature.response_speed * r.speed_reward_scale;
        let fatigue_penalty = if state.fatigue > r.fatigue_penalty_threshold {
            state.fatigue * r.fatigue_penalty_scale
        } else {
            0.0
        };
        let frustration_penalty = if state.motivation < r.frustration_penalty_threshold {
            (-state.motivation) * r.frustration_penalty_scale
        } else {
            0.0
        };

        let value = accuracy_reward + speed_reward - fatigue_penalty - frustration_penalty;

        Reward {
            value: value.clamp(-1.0, 1.0),
            components: RewardComponents {
                accuracy_reward,
                speed_reward,
                fatigue_penalty,
                frustration_penalty,
            },
        }
    }

    fn evaluate_objective(
        &self,
        reward: &Reward,
        retention_signal: f64,
        config: &AMASConfig,
    ) -> ObjectiveEvaluation {
        let w = &config.objective_weights;
        let score = reward.components.accuracy_reward * w.accuracy
            + reward.components.speed_reward * w.speed
            + retention_signal * w.retention
            - reward.components.fatigue_penalty * w.fatigue
            - reward.components.frustration_penalty * w.frustration;

        ObjectiveEvaluation {
            score,
            retention_gain: retention_signal,
            accuracy_gain: reward.components.accuracy_reward,
            speed_gain: reward.components.speed_reward,
            fatigue_penalty: reward.components.fatigue_penalty,
            frustration_penalty: reward.components.frustration_penalty,
        }
    }

    fn update_memory(
        &self,
        user_id: &str,
        raw_event: &RawEvent,
        feature: &FeatureVector,
        strategy: &StrategyParams,
        user_state: &UserState,
        config: &AMASConfig,
    ) -> Result<Option<WordMasteryDecision>, AppError> {
        if raw_event.word_id.is_empty() {
            return Ok(None);
        }

        let key = format!("mastery:{}", raw_event.word_id);
        let mut state = match self
            .store
            .get_engine_algo_state(user_id, &key)
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            Some(value) => match serde_json::from_value(value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(word_id = %raw_event.word_id, error = %e, "Mastery state deserialization failed, creating new");
                    mastery::WordMasteryState::new(&raw_event.word_id)
                }
            },
            None => mastery::WordMasteryState::new(&raw_event.word_id),
        };

        // B38: IAD - 计算混淆干扰惩罚，调整 interval_scale
        let mut adjusted_interval_scale = strategy.interval_scale;
        if config.feature_flags.iad_enabled {
            let iad_key = "iad";
            let mut iad_state: iad::IadState = self
                .store
                .get_engine_algo_state(user_id, iad_key)
                .map_err(|e| AppError::internal(&e.to_string()))?
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            let penalty = iad::interference_penalty(&raw_event.word_id, &iad_state, &config.iad);
            let factor = iad::interval_extension_factor(penalty, &config.iad);
            adjusted_interval_scale *= factor;

            // 记录混淆词对
            if let Some(confused_with) = &raw_event.confused_with {
                if !confused_with.is_empty() {
                    iad::record_confusion(
                        &mut iad_state,
                        &raw_event.word_id,
                        confused_with,
                        config.iad.confusion_decay_rate,
                        &config.iad,
                    );
                    if let Ok(val) = serde_json::to_value(&iad_state) {
                        let _ = self.store.set_engine_algo_state(user_id, iad_key, &val);
                    }
                }
            }
        }

        // B37: MTP - 计算词素迁移加成
        if config.feature_flags.mtp_enabled {
            let mtp_key = "mtp";
            let mut mtp_state: mtp::MtpState = self
                .store
                .get_engine_algo_state(user_id, mtp_key)
                .map_err(|e| AppError::internal(&e.to_string()))?
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            // 获取当前词的词素列表
            let morpheme_key =
                crate::store::keys::word_morpheme_key(&raw_event.word_id).unwrap_or_default();
            let word_morphemes: Vec<String> =
                if let Ok(Some(raw)) = self.store.word_morphemes.get(morpheme_key.as_bytes()) {
                    serde_json::from_slice::<serde_json::Value>(&raw)
                        .ok()
                        .and_then(|data| {
                            data.get("morphemes")
                                .and_then(|m| m.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| {
                                            v.get("text")
                                                .and_then(|t| t.as_str())
                                                .map(String::from)
                                        })
                                        .collect()
                                })
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };

            if !word_morphemes.is_empty() {
                // 计算词素迁移加成并应用到 interval_scale
                let bonus = mtp::morpheme_transfer_bonus(
                    &word_morphemes,
                    &mtp_state.known_morphemes,
                    &config.mtp,
                );
                if bonus > 0.0 {
                    adjusted_interval_scale *= 1.0 + bonus;
                }

                // 成功学习时更新已知词素
                if raw_event.is_correct {
                    mtp::update_known_morphemes(
                        &mut mtp_state,
                        &word_morphemes,
                        feature.quality,
                        &config.mtp,
                    );
                    if let Ok(val) = serde_json::to_value(&mtp_state) {
                        let _ = self.store.set_engine_algo_state(user_id, mtp_key, &val);
                    }
                }
            }
        }

        // B40: 自适应目标保持率
        let desired_retention = mdm::adaptive_desired_retention(
            config.memory_model.base_desired_retention,
            feature.accuracy,
            user_state.fatigue,
            user_state.motivation,
        );

        let decision = mastery::update_mastery(
            &mut state,
            raw_event.is_correct,
            feature.quality,
            adjusted_interval_scale,
            desired_retention,
            &config.memory_model,
        );

        self.store
            .set_engine_algo_state(
                user_id,
                &key,
                &serde_json::to_value(&state).map_err(|e| AppError::internal(&e.to_string()))?,
            )
            .map_err(|e| AppError::internal(&e.to_string()))?;

        Ok(Some(decision))
    }

    fn apply_constraints(
        &self,
        mut strategy: StrategyParams,
        state: &UserState,
        config: &AMASConfig,
    ) -> StrategyParams {
        let c = &config.constraints;

        if state.fatigue > c.high_fatigue_threshold {
            strategy.batch_size = strategy.batch_size.min(c.max_batch_size_when_fatigued);
            strategy.new_ratio = strategy.new_ratio.min(c.max_new_ratio_when_fatigued);
            strategy.difficulty = strategy.difficulty.min(c.max_difficulty_when_fatigued);
        }

        if state.attention < c.low_attention_threshold {
            strategy.review_mode = true;
            strategy.new_ratio = 0.0;
        }

        if state.motivation < c.low_motivation_threshold {
            strategy.difficulty =
                (strategy.difficulty - c.low_motivation_difficulty_drop).max(c.min_difficulty);
            strategy.new_ratio = (strategy.new_ratio - c.low_motivation_ratio_drop).max(0.0);
        }

        strategy.difficulty = strategy.difficulty.clamp(0.0, 1.0);
        strategy.new_ratio = strategy.new_ratio.clamp(0.0, 1.0);
        strategy.batch_size = strategy.batch_size.max(1);
        strategy.interval_scale = strategy.interval_scale.max(0.1);

        strategy
    }

    fn update_trust_scores(
        &self,
        algo_states: &mut AlgoStates,
        candidates: &[DecisionCandidate],
        reward: f64,
        user_state: &UserState,
        weights: &HashMap<AlgorithmId, f64>,
        config: &AMASConfig,
    ) {
        for candidate in candidates {
            let weight = weights.get(&candidate.algorithm_id).copied().unwrap_or(0.0);
            let learning_rate = config.feature.trust_base_learning_rate
                * (config.feature.trust_weight_blend + weight);
            ensemble::update_trust(
                &mut algo_states.trust_scores,
                candidate.algorithm_id,
                reward,
                learning_rate,
            );

            if candidate.algorithm_id == AlgorithmId::Ige {
                ige::update(&mut algo_states.ige, &candidate.strategy, reward);
            }

            if candidate.algorithm_id == AlgorithmId::Swd {
                swd::update(
                    &mut algo_states.swd,
                    user_state,
                    &candidate.strategy,
                    reward,
                    config,
                );
            }
        }
    }

    fn persist_state(
        &self,
        user_id: &str,
        user_state: &UserState,
        algo_states: &AlgoStates,
    ) -> Result<(), AppError> {
        // 在保存前清理浮点字段，防止 NaN 传播
        let mut state = user_state.clone();
        state.attention = sanitize_float(state.attention, 0.5).clamp(0.0, 1.0);
        state.fatigue = sanitize_float(state.fatigue, 0.0).clamp(0.0, 1.0);
        state.motivation = sanitize_float(state.motivation, 0.0).clamp(-1.0, 1.0);
        state.confidence = sanitize_float(state.confidence, 0.5).clamp(0.0, 1.0);
        state.cognitive_profile.memory_capacity =
            sanitize_float(state.cognitive_profile.memory_capacity, 0.5).clamp(0.0, 1.0);
        state.cognitive_profile.processing_speed =
            sanitize_float(state.cognitive_profile.processing_speed, 0.5).clamp(0.0, 1.0);
        state.cognitive_profile.stability =
            sanitize_float(state.cognitive_profile.stability, 0.5).clamp(0.0, 1.0);

        let user_state_json =
            serde_json::to_value(&state).map_err(|e| AppError::internal(&e.to_string()))?;

        let algo_entries: Vec<(String, serde_json::Value)> = vec![
            (
                "ige".to_string(),
                serde_json::to_value(&algo_states.ige)
                    .map_err(|e| AppError::internal(&e.to_string()))?,
            ),
            (
                "swd".to_string(),
                serde_json::to_value(&algo_states.swd)
                    .map_err(|e| AppError::internal(&e.to_string()))?,
            ),
            (
                "trust".to_string(),
                serde_json::to_value(&algo_states.trust_scores)
                    .map_err(|e| AppError::internal(&e.to_string()))?,
            ),
        ];

        self.store
            .persist_engine_state_atomic(user_id, &user_state_json, &algo_entries)
            .map_err(|e| AppError::internal(&e.to_string()))
    }

    fn build_explanation(
        &self,
        strategy: &StrategyParams,
        user_state: &UserState,
        weights: &HashMap<AlgorithmId, f64>,
    ) -> Explanation {
        let mut factors = Vec::new();
        factors.push(ExplanationFactor {
            name: "difficulty".to_string(),
            value: strategy.difficulty,
            impact: if strategy.difficulty > 0.5 {
                "positive".to_string()
            } else {
                "neutral".to_string()
            },
        });
        factors.push(ExplanationFactor {
            name: "fatigue".to_string(),
            value: user_state.fatigue,
            impact: if user_state.fatigue > 0.7 {
                "negative".to_string()
            } else {
                "neutral".to_string()
            },
        });

        for (algo, weight) in weights {
            factors.push(ExplanationFactor {
                name: format!("weight_{}", algo.as_str()),
                value: *weight,
                impact: "neutral".to_string(),
            });
        }

        Explanation {
            primary_reason: "Strategy generated by AMAS".to_string(),
            factors,
        }
    }

    fn emit_monitoring(
        &self,
        user_id: &str,
        session_id: &str,
        result: &ProcessResult,
        latency_ms: i64,
        config: &AMASConfig,
        pre_constraint_strategy: &StrategyParams,
    ) {
        monitoring::record_event(
            &self.store,
            user_id,
            session_id,
            result,
            latency_ms,
            config,
            pre_constraint_strategy,
        );
    }
}
