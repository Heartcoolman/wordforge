use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    pub ensemble_enabled: bool,
    pub heuristic_enabled: bool,
    pub ige_enabled: bool,
    pub swd_enabled: bool,
    pub mdm_enabled: bool,
    /// B38: Interference Aware Decay - 混淆词对干扰衰减
    #[serde(default)]
    pub iad_enabled: bool,
    /// B37: Morpheme Transfer Prediction - 词素迁移预测
    #[serde(default)]
    pub mtp_enabled: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            ensemble_enabled: true,
            heuristic_enabled: true,
            ige_enabled: true,
            swd_enabled: true,
            mdm_enabled: true,
            iad_enabled: false,
            mtp_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsembleConfig {
    pub base_weight_heuristic: f64,
    pub base_weight_ige: f64,
    pub base_weight_swd: f64,
    pub warmup_samples: u64,
    pub blend_scale: f64,
    pub blend_max: f64,
    pub min_weight: f64,
    #[serde(default = "default_warmup_heuristic_boost")]
    pub warmup_heuristic_boost: f64,
}

fn default_warmup_heuristic_boost() -> f64 {
    0.20
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        Self {
            base_weight_heuristic: 0.40,
            base_weight_ige: 0.30,
            base_weight_swd: 0.30,
            warmup_samples: 20,
            blend_scale: 100.0,
            blend_max: 0.50,
            min_weight: 0.15,
            warmup_heuristic_boost: 0.20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelingConfig {
    pub attention_smoothing: f64,
    pub confidence_decay: f64,
    pub min_confidence: f64,
    pub fatigue_increase_rate: f64,
    pub fatigue_recovery_rate: f64,
    pub motivation_momentum: f64,
    /// 视觉疲劳信号在混合公式中的权重 (0.0-1.0)
    /// 行为信号权重 = 1.0 - visual_fatigue_weight
    pub visual_fatigue_weight: f64,
    /// response_speed 归一化的最大响应时间（毫秒）
    #[serde(default = "default_response_speed_max_ms")]
    pub response_speed_max_ms: f64,
    /// 用户退出时的疲劳增加量
    #[serde(default = "default_fatigue_quit_increase")]
    pub fatigue_quit_increase: f64,
    /// engagement 中暂停次数的惩罚系数（每次暂停扣分）
    #[serde(default = "default_engagement_pause_penalty")]
    pub engagement_pause_penalty: f64,
    /// engagement 中暂停惩罚的上限
    #[serde(default = "default_engagement_pause_penalty_max")]
    pub engagement_pause_penalty_max: f64,
    /// engagement 中切换次数的惩罚系数（每次切换扣分）
    #[serde(default = "default_engagement_switch_penalty")]
    pub engagement_switch_penalty: f64,
    /// engagement 中切换惩罚的上限
    #[serde(default = "default_engagement_switch_penalty_max")]
    pub engagement_switch_penalty_max: f64,
    /// engagement 中焦点丢失时长的归一化基准（毫秒）
    #[serde(default = "default_engagement_focus_loss_base_ms")]
    pub engagement_focus_loss_base_ms: f64,
    /// engagement 中焦点丢失惩罚的上限
    #[serde(default = "default_engagement_focus_loss_penalty_max")]
    pub engagement_focus_loss_penalty_max: f64,
    /// 认知画像平滑系数（EMA alpha）
    #[serde(default = "default_cognitive_profile_alpha")]
    pub cognitive_profile_alpha: f64,
    /// 趋势状态平滑系数（EMA alpha）
    #[serde(default = "default_trend_alpha")]
    pub trend_alpha: f64,
}

fn default_response_speed_max_ms() -> f64 {
    10000.0
}
fn default_fatigue_quit_increase() -> f64 {
    0.2
}
fn default_engagement_pause_penalty() -> f64 {
    0.05
}
fn default_engagement_pause_penalty_max() -> f64 {
    0.3
}
fn default_engagement_switch_penalty() -> f64 {
    0.1
}
fn default_engagement_switch_penalty_max() -> f64 {
    0.3
}
fn default_engagement_focus_loss_base_ms() -> f64 {
    30000.0
}
fn default_engagement_focus_loss_penalty_max() -> f64 {
    0.3
}
fn default_cognitive_profile_alpha() -> f64 {
    0.1
}
fn default_trend_alpha() -> f64 {
    0.05
}

impl Default for ModelingConfig {
    fn default() -> Self {
        Self {
            attention_smoothing: 0.30,
            confidence_decay: 0.99,
            min_confidence: 0.10,
            fatigue_increase_rate: 0.02,
            fatigue_recovery_rate: 0.001,
            motivation_momentum: 0.1,
            visual_fatigue_weight: 0.4,
            response_speed_max_ms: 10000.0,
            fatigue_quit_increase: 0.2,
            engagement_pause_penalty: 0.05,
            engagement_pause_penalty_max: 0.3,
            engagement_switch_penalty: 0.1,
            engagement_switch_penalty_max: 0.3,
            engagement_focus_loss_base_ms: 30000.0,
            engagement_focus_loss_penalty_max: 0.3,
            cognitive_profile_alpha: 0.1,
            trend_alpha: 0.05,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintConfig {
    pub high_fatigue_threshold: f64,
    pub low_attention_threshold: f64,
    pub low_motivation_threshold: f64,
    pub max_batch_size_when_fatigued: u32,
    pub max_new_ratio_when_fatigued: f64,
    pub max_difficulty_when_fatigued: f64,
    #[serde(default = "default_low_motivation_difficulty_drop")]
    pub low_motivation_difficulty_drop: f64,
    #[serde(default = "default_low_motivation_ratio_drop")]
    pub low_motivation_ratio_drop: f64,
    #[serde(default = "default_min_difficulty")]
    pub min_difficulty: f64,
}

fn default_low_motivation_difficulty_drop() -> f64 {
    0.1
}
fn default_low_motivation_ratio_drop() -> f64 {
    0.1
}
fn default_min_difficulty() -> f64 {
    0.1
}

impl Default for ConstraintConfig {
    fn default() -> Self {
        Self {
            high_fatigue_threshold: 0.90,
            low_attention_threshold: 0.30,
            low_motivation_threshold: -0.50,
            max_batch_size_when_fatigued: 5,
            max_new_ratio_when_fatigued: 0.20,
            max_difficulty_when_fatigued: 0.55,
            low_motivation_difficulty_drop: 0.1,
            low_motivation_ratio_drop: 0.1,
            min_difficulty: 0.1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringConfig {
    pub sample_rate: f64,
    pub metrics_flush_interval_secs: u64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            sample_rate: 0.05,
            metrics_flush_interval_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColdStartConfig {
    pub classify_to_explore_events: u64,
    pub classify_to_explore_confidence: f64,
    pub explore_to_exploit_events: u64,
}

impl Default for ColdStartConfig {
    fn default() -> Self {
        Self {
            classify_to_explore_events: 20,
            classify_to_explore_confidence: 0.6,
            explore_to_exploit_events: 80,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifierConfig {
    pub fast_learner_threshold: f64,
    pub stable_learner_threshold: f64,
    pub processing_speed_weight: f64,
    pub memory_capacity_weight: f64,
    pub stability_weight: f64,
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self {
            fast_learner_threshold: 0.7,
            stable_learner_threshold: 0.4,
            processing_speed_weight: 0.4,
            memory_capacity_weight: 0.4,
            stability_weight: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveWeights {
    pub retention: f64,
    pub accuracy: f64,
    pub speed: f64,
    pub fatigue: f64,
    pub frustration: f64,
}

impl Default for ObjectiveWeights {
    fn default() -> Self {
        Self {
            retention: 0.35,
            accuracy: 0.25,
            speed: 0.15,
            fatigue: 0.15,
            frustration: 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewardConfig {
    pub speed_reward_scale: f64,
    pub fatigue_penalty_threshold: f64,
    pub fatigue_penalty_scale: f64,
    pub frustration_penalty_threshold: f64,
    pub frustration_penalty_scale: f64,
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            speed_reward_scale: 0.5,
            fatigue_penalty_threshold: 0.7,
            fatigue_penalty_scale: 0.3,
            frustration_penalty_threshold: -0.3,
            frustration_penalty_scale: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureConfig {
    pub hint_penalty: f64,
    pub quality_accuracy_weight: f64,
    pub quality_speed_weight: f64,
    pub motivation_positive_signal: f64,
    pub motivation_negative_signal: f64,
    pub confidence_positive_signal: f64,
    pub confidence_negative_signal: f64,
    pub temporal_profile_alpha: f64,
    pub temporal_boost_base: f64,
    pub temporal_boost_scale: f64,
    pub temporal_boost_min: f64,
    pub temporal_boost_max: f64,
    pub trust_base_learning_rate: f64,
    pub trust_weight_blend: f64,
    #[serde(default = "default_incorrect_quality_scale")]
    pub incorrect_quality_scale: f64,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            hint_penalty: 0.3,
            quality_accuracy_weight: 0.6,
            quality_speed_weight: 0.4,
            motivation_positive_signal: 0.1,
            motivation_negative_signal: -0.15,
            confidence_positive_signal: 0.02,
            confidence_negative_signal: -0.02,
            temporal_profile_alpha: 0.3,
            temporal_boost_base: 0.7,
            temporal_boost_scale: 0.6,
            temporal_boost_min: 0.5,
            temporal_boost_max: 1.5,
            trust_base_learning_rate: 0.05,
            trust_weight_blend: 0.5,
            incorrect_quality_scale: 0.1,
        }
    }
}

fn default_incorrect_quality_scale() -> f64 {
    0.1
}

// --- 以下为热重载子配置结构体 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EloConfig {
    pub k_factor: f64,
    pub novice_k_multiplier: f64,
    pub novice_game_threshold: u32,
    pub default_elo: f64,
    pub zpd_optimal_offset: f64,
    pub zpd_gaussian_sigma: f64,
    #[serde(default = "default_min_elo")]
    pub min_elo: f64,
    #[serde(default = "default_max_elo")]
    pub max_elo: f64,
    #[serde(default = "default_word_k_factor_ratio")]
    pub word_k_factor_ratio: f64,
}

fn default_word_k_factor_ratio() -> f64 {
    0.5
}

fn default_min_elo() -> f64 {
    400.0
}
fn default_max_elo() -> f64 {
    2400.0
}

impl Default for EloConfig {
    fn default() -> Self {
        Self {
            k_factor: 32.0,
            novice_k_multiplier: 2.0,
            novice_game_threshold: 30,
            default_elo: 1200.0,
            zpd_optimal_offset: 100.0,
            zpd_gaussian_sigma: 150.0,
            min_elo: 400.0,
            max_elo: 2400.0,
            word_k_factor_ratio: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FatigueDecayConfig {
    pub full_reset_threshold_secs: f64,
    pub decay_start_threshold_secs: f64,
    pub decay_time_constant_secs: f64,
}

impl Default for FatigueDecayConfig {
    fn default() -> Self {
        Self {
            full_reset_threshold_secs: 1800.0,
            decay_start_threshold_secs: 300.0,
            decay_time_constant_secs: 600.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeuristicConfig {
    pub cold_start_event_threshold: u64,
    pub cold_start_difficulty: f64,
    pub cold_start_batch_size: u32,
    pub cold_start_new_ratio: f64,
    pub accuracy_speed_difficulty_boost: f64,
    pub low_accuracy_difficulty_drop: f64,
    pub low_accuracy_ratio_drop: f64,
    pub low_motivation_difficulty_drop: f64,
    pub low_motivation_max_batch: u32,
    pub confidence_base: f64,
    pub confidence_decay_cap: f64,
    pub confidence_min: f64,
    pub confidence_decay_scale: f64,
}

impl Default for HeuristicConfig {
    fn default() -> Self {
        Self {
            cold_start_event_threshold: 10,
            cold_start_difficulty: 0.3,
            cold_start_batch_size: 5,
            cold_start_new_ratio: 0.5,
            accuracy_speed_difficulty_boost: 0.1,
            low_accuracy_difficulty_drop: 0.15,
            low_accuracy_ratio_drop: 0.1,
            low_motivation_difficulty_drop: 0.1,
            low_motivation_max_batch: 8,
            confidence_base: 0.7,
            confidence_decay_cap: 0.5,
            confidence_min: 0.2,
            confidence_decay_scale: 200.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgeConfig {
    pub batch_size: u32,
    pub interval_scale: f64,
    pub ucb_confidence_coeff: f64,
    pub default_confidence: f64,
}

impl Default for IgeConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            interval_scale: 1.0,
            ucb_confidence_coeff: 2.0,
            default_confidence: 0.6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwdConfig {
    pub max_history_size: usize,
    pub history_filter_threshold: f64,
    pub fallback_confidence: f64,
    #[serde(default = "default_similarity_cache_ttl_secs")]
    pub similarity_cache_ttl_secs: u64,
}

fn default_similarity_cache_ttl_secs() -> u64 {
    300
}

impl Default for SwdConfig {
    fn default() -> Self {
        Self {
            max_history_size: 200,
            history_filter_threshold: -0.5,
            fallback_confidence: 0.2,
            similarity_cache_ttl_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryModelConfig {
    pub short_term_learning_rate: f64,
    pub medium_term_learning_rate: f64,
    pub long_term_learning_rate: f64,
    pub composite_weight_short: f64,
    pub composite_weight_medium: f64,
    pub composite_weight_long: f64,
    pub consolidation_rate_scale: f64,
    pub consolidation_bonus: f64,
    pub mastery_composite_threshold: f64,
    pub mastery_accuracy_threshold: f64,
    pub mastery_streak_threshold: u32,
    pub reviewing_threshold: f64,
    pub half_life_base_epsilon: f64,
    pub half_life_time_unit_secs: f64,
    pub recall_risk_bonus: f64,
    pub recall_risk_threshold: f64,
    #[serde(default = "default_base_desired_retention")]
    pub base_desired_retention: f64,
    #[serde(default = "default_passive_decay_half_life_days")]
    pub passive_decay_half_life_days: f64,
    #[serde(default = "default_passive_decay_power")]
    pub passive_decay_power: f64,
    #[serde(default = "default_mastery_window_size")]
    pub mastery_window_size: u32,
}

fn default_base_desired_retention() -> f64 {
    0.85
}
fn default_passive_decay_half_life_days() -> f64 {
    30.0
}
fn default_passive_decay_power() -> f64 {
    0.5
}
fn default_mastery_window_size() -> u32 {
    20
}

impl Default for MemoryModelConfig {
    fn default() -> Self {
        Self {
            short_term_learning_rate: 0.50,
            medium_term_learning_rate: 0.20,
            long_term_learning_rate: 0.05,
            composite_weight_short: 0.20,
            composite_weight_medium: 0.30,
            composite_weight_long: 0.50,
            consolidation_rate_scale: 0.03,
            consolidation_bonus: 0.2,
            mastery_composite_threshold: 0.8,
            mastery_accuracy_threshold: 0.9,
            mastery_streak_threshold: 3,
            reviewing_threshold: 0.4,
            half_life_base_epsilon: 0.1,
            half_life_time_unit_secs: 86400.0,
            recall_risk_bonus: 0.2,
            recall_risk_threshold: 0.5,
            base_desired_retention: 0.85,
            passive_decay_half_life_days: 30.0,
            passive_decay_power: 0.5,
            mastery_window_size: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IadConfig {
    pub interference_penalty_factor: f64,
    pub interference_penalty_cap: f64,
    pub max_confusion_pairs: usize,
    pub new_confusion_initial_score: f64,
    pub confusion_update_increment: f64,
    pub interval_shortening_factor: f64,
    #[serde(default = "default_confusion_decay_rate")]
    pub confusion_decay_rate: f64,
}

fn default_confusion_decay_rate() -> f64 {
    0.05
}

impl Default for IadConfig {
    fn default() -> Self {
        Self {
            interference_penalty_factor: 0.1,
            interference_penalty_cap: 0.3,
            max_confusion_pairs: 20,
            new_confusion_initial_score: 0.2,
            confusion_update_increment: 0.2,
            interval_shortening_factor: 0.5,
            confusion_decay_rate: 0.05,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MtpConfig {
    pub morpheme_transfer_coeff: f64,
    pub morpheme_bonus_cap: f64,
    pub known_morpheme_decay: f64,
    pub new_morpheme_initial_coeff: f64,
    pub max_known_morphemes: usize,
}

impl Default for MtpConfig {
    fn default() -> Self {
        Self {
            morpheme_transfer_coeff: 0.15,
            morpheme_bonus_cap: 0.3,
            known_morpheme_decay: 0.9,
            new_morpheme_initial_coeff: 0.5,
            max_known_morphemes: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordSelectorConfig {
    pub review_ucb_weight: f64,
    pub review_ucb_max_bonus: f64,
    pub new_word_gaussian_sigma: f64,
    pub error_prone_bonus: f64,
    pub recently_mastered_bonus: f64,
    pub recall_mastered_threshold: f64,
    #[serde(default = "default_sigmoid_steepness")]
    pub sigmoid_steepness: f64,
}

fn default_sigmoid_steepness() -> f64 {
    8.0
}

impl Default for WordSelectorConfig {
    fn default() -> Self {
        Self {
            review_ucb_weight: 0.18,
            review_ucb_max_bonus: 0.35,
            new_word_gaussian_sigma: 0.3,
            error_prone_bonus: 0.3,
            recently_mastered_bonus: 0.15,
            recall_mastered_threshold: 0.7,
            sigmoid_steepness: 8.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterventionConfig {
    pub fatigue_alert_threshold: f64,
    pub motivation_alert_threshold: f64,
    pub attention_alert_threshold: f64,
}

impl Default for InterventionConfig {
    fn default() -> Self {
        Self {
            fatigue_alert_threshold: 0.7,
            motivation_alert_threshold: -0.3,
            attention_alert_threshold: 0.3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningStrategyConfig {
    pub cross_session_high_accuracy: f64,
    pub cross_session_medium_accuracy: f64,
    pub cross_session_high_difficulty: f64,
    pub cross_session_medium_difficulty: f64,
    pub cross_session_low_difficulty: f64,
    pub session_boost_accuracy: f64,
    pub session_drop_accuracy: f64,
    pub difficulty_boost_step: f64,
    pub difficulty_drop_step: f64,
    pub ratio_boost_step: f64,
    pub ratio_drop_step: f64,
    pub sprint_mastery_ratio: f64,
    pub sprint_new_ratio: f64,
    pub confidence_boost_threshold: f64,
    pub confidence_difficulty_boost: f64,
    pub motivation_ratio_threshold: f64,
    pub motivation_ratio_boost: f64,
    pub fatigue_reduction_threshold: f64,
    pub fatigue_batch_scale: f64,
    pub fatigue_difficulty_drop: f64,
}

impl Default for LearningStrategyConfig {
    fn default() -> Self {
        Self {
            cross_session_high_accuracy: 0.8,
            cross_session_medium_accuracy: 0.5,
            cross_session_high_difficulty: 0.6,
            cross_session_medium_difficulty: 0.5,
            cross_session_low_difficulty: 0.35,
            session_boost_accuracy: 0.8,
            session_drop_accuracy: 0.4,
            difficulty_boost_step: 0.1,
            difficulty_drop_step: 0.15,
            ratio_boost_step: 0.15,
            ratio_drop_step: 0.15,
            sprint_mastery_ratio: 0.8,
            sprint_new_ratio: 0.9,
            confidence_boost_threshold: 0.5,
            confidence_difficulty_boost: 0.1,
            motivation_ratio_threshold: 0.3,
            motivation_ratio_boost: 0.1,
            fatigue_reduction_threshold: 0.5,
            fatigue_batch_scale: 0.7,
            fatigue_difficulty_drop: 0.15,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AMASConfig {
    pub feature_flags: FeatureFlags,
    pub ensemble: EnsembleConfig,
    pub modeling: ModelingConfig,
    pub constraints: ConstraintConfig,
    pub monitoring: MonitoringConfig,
    pub cold_start: ColdStartConfig,
    pub objective_weights: ObjectiveWeights,
    #[serde(default)]
    pub reward: RewardConfig,
    #[serde(default)]
    pub feature: FeatureConfig,
    #[serde(default)]
    pub elo: EloConfig,
    #[serde(default)]
    pub fatigue_decay: FatigueDecayConfig,
    #[serde(default)]
    pub heuristic: HeuristicConfig,
    #[serde(default)]
    pub ige: IgeConfig,
    #[serde(default)]
    pub swd: SwdConfig,
    #[serde(default)]
    pub memory_model: MemoryModelConfig,
    #[serde(default)]
    pub iad: IadConfig,
    #[serde(default)]
    pub mtp: MtpConfig,
    #[serde(default)]
    pub word_selector: WordSelectorConfig,
    #[serde(default)]
    pub intervention: InterventionConfig,
    #[serde(default)]
    pub learning_strategy: LearningStrategyConfig,
    #[serde(default)]
    pub classifier: ClassifierConfig,
}

impl AMASConfig {
    pub fn from_env(env_config: &crate::config::AMASEnvConfig) -> Self {
        let mut config = Self::default();
        config.feature_flags.ensemble_enabled = env_config.ensemble_enabled;
        config.monitoring.sample_rate = env_config.monitor_sample_rate;
        config
    }

    pub fn validate(&self) -> Result<(), String> {
        // ModelingConfig 参数范围检查
        if !(0.0..=1.0).contains(&self.modeling.attention_smoothing) {
            return Err("modeling.attention_smoothing must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.fatigue_increase_rate) {
            return Err("modeling.fatigue_increase_rate must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.confidence_decay) {
            return Err("modeling.confidence_decay must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.min_confidence) {
            return Err("modeling.min_confidence must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.motivation_momentum) {
            return Err("modeling.motivation_momentum must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.fatigue_recovery_rate) {
            return Err("modeling.fatigue_recovery_rate must be in [0,1]".to_string());
        }
        if self.modeling.response_speed_max_ms <= 0.0 {
            return Err("modeling.response_speed_max_ms must be > 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.fatigue_quit_increase) {
            return Err("modeling.fatigue_quit_increase must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.cognitive_profile_alpha) {
            return Err("modeling.cognitive_profile_alpha must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.trend_alpha) {
            return Err("modeling.trend_alpha must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.monitoring.sample_rate) {
            return Err("monitoring.sample_rate must be in [0,1]".to_string());
        }

        if !(0.0..=1.0).contains(&self.constraints.high_fatigue_threshold)
            || !(0.0..=1.0).contains(&self.constraints.low_attention_threshold)
            || !(-1.0..=1.0).contains(&self.constraints.low_motivation_threshold)
        {
            return Err("invalid constraint thresholds".to_string());
        }

        if self.ensemble.base_weight_heuristic <= 0.0
            || self.ensemble.base_weight_ige <= 0.0
            || self.ensemble.base_weight_swd <= 0.0
        {
            return Err("ensemble base weights must be > 0".to_string());
        }

        let base_weight_sum = self.ensemble.base_weight_heuristic
            + self.ensemble.base_weight_ige
            + self.ensemble.base_weight_swd;
        if (base_weight_sum - 1.0).abs() > 0.01 {
            return Err(format!(
                "ensemble base weights should sum to ~1.0 (got {base_weight_sum:.3})"
            ));
        }

        if !(0.0..=1.0).contains(&self.modeling.visual_fatigue_weight) {
            return Err("modeling.visual_fatigue_weight must be in [0,1]".to_string());
        }

        if self.ensemble.min_weight <= 0.0 || self.ensemble.min_weight > 1.0 {
            return Err("ensemble.min_weight must be in (0,1]".to_string());
        }

        if 3.0 * self.ensemble.min_weight > 1.0 {
            return Err("ensemble.min_weight too large: 3 * min_weight must be <= 1.0".to_string());
        }

        if self.objective_weights.retention < 0.0
            || self.objective_weights.accuracy < 0.0
            || self.objective_weights.speed < 0.0
            || self.objective_weights.fatigue < 0.0
            || self.objective_weights.frustration < 0.0
        {
            return Err("objective_weights must be >= 0".to_string());
        }

        let sum = self.objective_weights.retention
            + self.objective_weights.accuracy
            + self.objective_weights.speed
            + self.objective_weights.fatigue
            + self.objective_weights.frustration;
        if sum <= 0.0 {
            return Err("objective_weights sum must be > 0".to_string());
        }
        if (sum - 1.0).abs() > 0.05 {
            return Err(format!(
                "objective_weights should sum to ~1.0 (got {sum:.3}). Normalize or adjust weights."
            ));
        }

        // FeatureConfig
        if !(0.0..=1.0).contains(&self.feature.hint_penalty) {
            return Err("feature.hint_penalty must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.feature.quality_accuracy_weight) {
            return Err("feature.quality_accuracy_weight must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.feature.quality_speed_weight) {
            return Err("feature.quality_speed_weight must be in [0,1]".to_string());
        }
        if !(0.001..=1.0).contains(&self.feature.trust_base_learning_rate) {
            return Err("feature.trust_base_learning_rate must be in [0.001,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.feature.incorrect_quality_scale) {
            return Err("feature.incorrect_quality_scale must be in [0,1]".to_string());
        }

        // RewardConfig
        if self.reward.speed_reward_scale < 0.0 || self.reward.speed_reward_scale > 10.0 {
            return Err("reward.speed_reward_scale must be in [0,10]".to_string());
        }
        if self.reward.fatigue_penalty_scale < 0.0 || self.reward.fatigue_penalty_scale > 10.0 {
            return Err("reward.fatigue_penalty_scale must be in [0,10]".to_string());
        }
        if self.reward.frustration_penalty_threshold > 0.0 {
            return Err("reward.frustration_penalty_threshold must be <= 0".to_string());
        }

        // ModelingConfig - engagement penalties
        if self.modeling.engagement_pause_penalty < 0.0
            || self.modeling.engagement_pause_penalty > 1.0
        {
            return Err("modeling.engagement_pause_penalty must be in [0,1]".to_string());
        }

        // EloConfig
        if self.elo.k_factor <= 0.0 {
            return Err("elo.k_factor must be > 0".to_string());
        }
        if self.elo.min_elo >= self.elo.max_elo {
            return Err("elo.min_elo must be < elo.max_elo".to_string());
        }
        if self.elo.novice_k_multiplier <= 0.0 {
            return Err("elo.novice_k_multiplier must be > 0".to_string());
        }
        if self.elo.zpd_gaussian_sigma <= 0.0 {
            return Err("elo.zpd_gaussian_sigma must be > 0".to_string());
        }

        // FatigueDecayConfig
        if self.fatigue_decay.full_reset_threshold_secs
            <= self.fatigue_decay.decay_start_threshold_secs
        {
            return Err(
                "fatigue_decay.full_reset_threshold_secs must be > decay_start_threshold_secs"
                    .to_string(),
            );
        }
        if self.fatigue_decay.decay_start_threshold_secs <= 0.0 {
            return Err("fatigue_decay.decay_start_threshold_secs must be > 0".to_string());
        }
        if self.fatigue_decay.decay_time_constant_secs <= 0.0 {
            return Err("fatigue_decay.decay_time_constant_secs must be > 0".to_string());
        }

        // HeuristicConfig
        if !(0.0..=1.0).contains(&self.heuristic.cold_start_difficulty) {
            return Err("heuristic.cold_start_difficulty must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.heuristic.cold_start_new_ratio) {
            return Err("heuristic.cold_start_new_ratio must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.heuristic.confidence_base) {
            return Err("heuristic.confidence_base must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.heuristic.confidence_min) {
            return Err("heuristic.confidence_min must be in [0,1]".to_string());
        }
        if self.heuristic.confidence_decay_scale <= 0.0 {
            return Err("heuristic.confidence_decay_scale must be > 0".to_string());
        }

        // IgeConfig
        if self.ige.ucb_confidence_coeff <= 0.0 {
            return Err("ige.ucb_confidence_coeff must be > 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.ige.default_confidence) {
            return Err("ige.default_confidence must be in [0,1]".to_string());
        }

        // SwdConfig
        if self.swd.max_history_size == 0 {
            return Err("swd.max_history_size must be > 0".to_string());
        }

        // MemoryModelConfig
        if !(0.0..=1.0).contains(&self.memory_model.short_term_learning_rate) {
            return Err("memory_model.short_term_learning_rate must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_model.medium_term_learning_rate) {
            return Err("memory_model.medium_term_learning_rate must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_model.long_term_learning_rate) {
            return Err("memory_model.long_term_learning_rate must be in [0,1]".to_string());
        }
        let composite_sum = self.memory_model.composite_weight_short
            + self.memory_model.composite_weight_medium
            + self.memory_model.composite_weight_long;
        if (composite_sum - 1.0).abs() > 0.01 {
            return Err("memory_model composite weights must sum to ~1.0".to_string());
        }
        if self.memory_model.half_life_time_unit_secs <= 0.0 {
            return Err("memory_model.half_life_time_unit_secs must be > 0".to_string());
        }
        if self.memory_model.half_life_base_epsilon <= 0.0 {
            return Err("memory_model.half_life_base_epsilon must be > 0".to_string());
        }
        if !(0.5..=0.99).contains(&self.memory_model.base_desired_retention) {
            return Err("memory_model.base_desired_retention must be in [0.5,0.99]".to_string());
        }
        if self.memory_model.consolidation_bonus < 0.0 {
            return Err("memory_model.consolidation_bonus must be >= 0".to_string());
        }
        if self.memory_model.passive_decay_half_life_days <= 0.0 {
            return Err("memory_model.passive_decay_half_life_days must be > 0".to_string());
        }
        if self.memory_model.passive_decay_power <= 0.0 {
            return Err("memory_model.passive_decay_power must be > 0".to_string());
        }
        if self.memory_model.mastery_window_size == 0 {
            return Err("memory_model.mastery_window_size must be > 0".to_string());
        }

        // IadConfig
        if !(0.0..=1.0).contains(&self.iad.interference_penalty_factor) {
            return Err("iad.interference_penalty_factor must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.iad.interference_penalty_cap) {
            return Err("iad.interference_penalty_cap must be in [0,1]".to_string());
        }

        // MtpConfig
        if !(0.0..=1.0).contains(&self.mtp.morpheme_transfer_coeff) {
            return Err("mtp.morpheme_transfer_coeff must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.mtp.morpheme_bonus_cap) {
            return Err("mtp.morpheme_bonus_cap must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.mtp.known_morpheme_decay) {
            return Err("mtp.known_morpheme_decay must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.mtp.new_morpheme_initial_coeff) {
            return Err("mtp.new_morpheme_initial_coeff must be in [0,1]".to_string());
        }

        // WordSelectorConfig
        if self.word_selector.review_ucb_weight < 0.0 {
            return Err("word_selector.review_ucb_weight must be >= 0".to_string());
        }
        if self.word_selector.review_ucb_max_bonus < 0.0 {
            return Err("word_selector.review_ucb_max_bonus must be >= 0".to_string());
        }
        if self.word_selector.new_word_gaussian_sigma <= 0.0 {
            return Err("word_selector.new_word_gaussian_sigma must be > 0".to_string());
        }

        // InterventionConfig
        if !(0.0..=1.0).contains(&self.intervention.fatigue_alert_threshold) {
            return Err("intervention.fatigue_alert_threshold must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.intervention.attention_alert_threshold) {
            return Err("intervention.attention_alert_threshold must be in [0,1]".to_string());
        }

        // LearningStrategyConfig
        if !(0.0..=1.0).contains(&self.learning_strategy.cross_session_high_accuracy) {
            return Err(
                "learning_strategy.cross_session_high_accuracy must be in [0,1]".to_string(),
            );
        }
        if !(0.0..=1.0).contains(&self.learning_strategy.sprint_mastery_ratio) {
            return Err("learning_strategy.sprint_mastery_ratio must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.learning_strategy.fatigue_reduction_threshold) {
            return Err(
                "learning_strategy.fatigue_reduction_threshold must be in [0,1]".to_string(),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = AMASConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn invalid_config_is_rejected() {
        let mut cfg = AMASConfig::default();
        cfg.monitoring.sample_rate = 2.0;
        assert!(cfg.validate().is_err());
    }
}
