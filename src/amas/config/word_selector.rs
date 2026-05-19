use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    #[serde(default = "default_spacing_cooldown_secs")]
    pub spacing_cooldown_secs: f64,
    #[serde(default = "default_optimal_recall_center")]
    pub optimal_recall_center: f64,
    #[serde(default = "default_optimal_recall_sigma")]
    pub optimal_recall_sigma: f64,
}

pub(crate) fn default_sigmoid_steepness() -> f64 {
    8.0
}
pub(crate) fn default_spacing_cooldown_secs() -> f64 {
    300.0
}
pub(crate) fn default_optimal_recall_center() -> f64 {
    0.50
}
pub(crate) fn default_optimal_recall_sigma() -> f64 {
    0.30
}

impl Default for WordSelectorConfig {
    fn default() -> Self {
        Self {
            review_ucb_weight: 0.12,
            review_ucb_max_bonus: 0.35,
            new_word_gaussian_sigma: 0.3,
            error_prone_bonus: 0.3,
            recently_mastered_bonus: 0.15,
            recall_mastered_threshold: 0.7,
            sigmoid_steepness: 8.0,
            spacing_cooldown_secs: 300.0,
            optimal_recall_center: 0.50,
            optimal_recall_sigma: 0.30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
