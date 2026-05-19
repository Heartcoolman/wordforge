use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

pub(crate) fn default_low_motivation_difficulty_drop() -> f64 {
    0.1
}
pub(crate) fn default_low_motivation_ratio_drop() -> f64 {
    0.1
}
pub(crate) fn default_min_difficulty() -> f64 {
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RewardConfig {
    pub speed_reward_scale: f64,
    pub fatigue_penalty_threshold: f64,
    pub fatigue_penalty_scale: f64,
    pub frustration_penalty_threshold: f64,
    pub frustration_penalty_scale: f64,
    #[serde(default = "default_expected_forget_cost_weight")]
    pub expected_forget_cost_weight: f64,
}

pub(crate) fn default_expected_forget_cost_weight() -> f64 {
    0.3
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            speed_reward_scale: 0.5,
            fatigue_penalty_threshold: 0.7,
            fatigue_penalty_scale: 0.3,
            frustration_penalty_threshold: -0.3,
            frustration_penalty_scale: 0.2,
            expected_forget_cost_weight: 0.3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

pub(crate) fn default_incorrect_quality_scale() -> f64 {
    0.1
}

// --- 以下为热重载子配置结构体 ---
