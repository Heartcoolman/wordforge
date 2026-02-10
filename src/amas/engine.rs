use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, RwLock};

use crate::amas::config::AMASConfig;
use crate::amas::decision::{ensemble, heuristic, ige, swd};
use crate::amas::memory::mastery;
use crate::amas::metrics;
use crate::amas::monitoring;
use crate::amas::types::*;
use crate::response::AppError;
use crate::store::Store;

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

        // Periodically prune entries that are no longer held by anyone.
        // Arc::strong_count == 1 means only the HashMap holds a reference,
        // so the lock is idle and can be safely removed.
        if locks.len() > 1000 {
            locks.retain(|_, v| Arc::strong_count(v) > 1);
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

        let feature = self.build_feature_vector(&raw_event, &user_state);
        self.update_modeling(&mut user_state, &feature, &config);

        let cold_start_phase = self.determine_cold_start_phase(&user_state, &config);

        let candidates = self.generate_candidates(&user_state, &feature, &mut algo_states, &config);
        let (final_strategy, weights) =
            self.ensemble_or_fallback(&candidates, &user_state, &algo_states, &config);

        let reward = self.compute_reward(&feature, &user_state, &config);
        let word_mastery = self.update_memory(user_id, &raw_event, &feature, &final_strategy)?;

        let retention_signal = word_mastery
            .as_ref()
            .map(|wm| wm.recall_probability)
            .unwrap_or(0.0);
        let _objective = self.evaluate_objective(&reward, retention_signal, &config);

        let constrained_strategy = self.apply_constraints(final_strategy.clone(), &user_state, &config);

        self.update_trust_scores(&mut algo_states, &candidates, reward.value, &user_state, &weights);

        user_state.session_event_count += 1;
        user_state.total_event_count += 1;
        user_state.last_active_at = Some(chrono::Utc::now());

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
        self.emit_monitoring(user_id, &session_id, &result, latency_ms, &config, &final_strategy);

        Ok(result)
    }

    pub fn get_user_state(&self, user_id: &str) -> Result<UserState, AppError> {
        self.load_or_init_state(user_id)
    }

    pub fn compute_strategy_from_state(&self, user_state: &UserState) -> StrategyParams {
        let mut strategy = StrategyParams::default();

        // Adjust difficulty based on user confidence and motivation
        if user_state.confidence > 0.5 {
            strategy.difficulty = (strategy.difficulty + 0.1).min(1.0);
        }
        if user_state.motivation > 0.3 {
            strategy.new_ratio = (strategy.new_ratio + 0.1).min(1.0);
        }
        if user_state.fatigue > 0.5 {
            strategy.batch_size = (strategy.batch_size as f64 * 0.7).max(3.0) as u32;
            strategy.difficulty = (strategy.difficulty - 0.15).max(0.1);
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
            .set_engine_user_state(user_id, &serde_json::to_value(&UserState::default())
                .map_err(|e| AppError::internal(&e.to_string()))?)
            .map_err(|e| AppError::internal(&e.to_string()))?;

        // Clear algorithm states
        for algo in &["ige", "swd", "trust"] {
            let key = crate::store::keys::engine_algo_state_key(user_id, algo);
            let _ = self.store.engine_algorithm_states.remove(key.as_bytes());
        }

        Ok(())
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

    fn build_feature_vector(&self, event: &RawEvent, state: &UserState) -> FeatureVector {
        let accuracy = if event.is_correct { 1.0 } else { 0.0 };
        let response_speed = (1.0 - (event.response_time_ms as f64 / 10000.0)).clamp(0.0, 1.0);
        let hint_penalty = if event.hint_used { 0.3 } else { 0.0 };
        let quality = (accuracy * 0.6 + response_speed * 0.4 - hint_penalty).clamp(0.0, 1.0);
        let engagement = Self::compute_engagement(event);

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

    fn compute_engagement(event: &RawEvent) -> f64 {
        let mut score = 1.0;
        if let Some(pause) = event.pause_count {
            score -= (pause.max(0) as f64 * 0.05).min(0.3);
        }
        if let Some(switch) = event.switch_count {
            score -= (switch.max(0) as f64 * 0.1).min(0.3);
        }
        if let Some(focus_loss) = event.focus_loss_duration_ms {
            score -= (focus_loss.max(0) as f64 / 30000.0).min(0.3);
        }
        score.clamp(0.0, 1.0)
    }

    fn update_modeling(&self, state: &mut UserState, feature: &FeatureVector, config: &AMASConfig) {
        let m = &config.modeling;

        state.attention = state.attention * (1.0 - m.attention_smoothing)
            + feature.engagement * m.attention_smoothing;
        state.attention = state.attention.clamp(0.0, 1.0);

        if feature.is_quit {
            state.fatigue = (state.fatigue + 0.2).min(1.0);
        } else {
            state.fatigue = (state.fatigue + m.fatigue_increase_rate).min(1.0);
        }

        // B27: Fatigue time decay
        if feature.time_since_last_event_secs >= 1800.0 {
            // >= 30 min: full reset
            state.fatigue = 0.0;
        } else if feature.time_since_last_event_secs > 300.0 {
            // 5-30 min: exponential decay
            let decay_factor = (-feature.time_since_last_event_secs / 600.0).exp();
            state.fatigue *= decay_factor;
        }

        let motivation_signal = if feature.accuracy > 0.5 { 0.1 } else { -0.15 };
        state.motivation = state.motivation * (1.0 - m.motivation_momentum)
            + motivation_signal * m.motivation_momentum;
        state.motivation = state.motivation.clamp(-1.0, 1.0);

        let confidence_signal = if feature.quality > 0.5 { 0.02 } else { -0.02 };
        state.confidence =
            (state.confidence * m.confidence_decay + confidence_signal).clamp(m.min_confidence, 1.0);

        // B25: Update cognitive profile
        let alpha = 0.1;
        state.cognitive_profile.processing_speed = state.cognitive_profile.processing_speed * (1.0 - alpha)
            + feature.response_speed * alpha;
        state.cognitive_profile.memory_capacity = state.cognitive_profile.memory_capacity * (1.0 - alpha)
            + feature.accuracy * alpha;
        state.cognitive_profile.stability = state.cognitive_profile.stability * (1.0 - alpha)
            + feature.quality * alpha;

        // B25: Update trend state
        let trend_alpha = 0.05;
        state.trend_state.accuracy_trend = state.trend_state.accuracy_trend * (1.0 - trend_alpha)
            + (feature.accuracy - 0.5) * trend_alpha;
        state.trend_state.speed_trend = state.trend_state.speed_trend * (1.0 - trend_alpha)
            + (feature.response_speed - 0.5) * trend_alpha;
        state.trend_state.engagement_trend = state.trend_state.engagement_trend * (1.0 - trend_alpha)
            + (feature.engagement - 0.5) * trend_alpha;
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
        let state = self.load_or_init_state(user_id)?;
        let cp = &state.cognitive_profile;

        // Simple AUC-like classifier based on cognitive profile
        let auc = cp.processing_speed * 0.4 + cp.memory_capacity * 0.4 + cp.stability * 0.2;
        if auc > 0.7 {
            Ok(LearnerType::Fast)
        } else if auc > 0.4 {
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
        _config: &AMASConfig,
    ) -> Reward {
        let accuracy_reward = feature.accuracy;
        let speed_reward = feature.response_speed * 0.5;
        let fatigue_penalty = if state.fatigue > 0.7 {
            state.fatigue * 0.3
        } else {
            0.0
        };
        let frustration_penalty = if state.motivation < -0.3 {
            (-state.motivation) * 0.2
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

        let decision = mastery::update_mastery(
            &mut state,
            raw_event.is_correct,
            feature.quality,
            strategy.interval_scale,
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
            strategy.difficulty = (strategy.difficulty - 0.1).max(0.1);
            strategy.new_ratio = (strategy.new_ratio - 0.1).max(0.0);
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
    ) {
        for candidate in candidates {
            let _weight = weights.get(&candidate.algorithm_id).copied().unwrap_or(0.0);
            ensemble::update_trust(
                &mut algo_states.trust_scores,
                candidate.algorithm_id,
                reward,
                0.05,
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
        let user_state_json =
            serde_json::to_value(user_state).map_err(|e| AppError::internal(&e.to_string()))?;

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
        monitoring::record_event(&self.store, user_id, session_id, result, latency_ms, config, pre_constraint_strategy);
    }
}
