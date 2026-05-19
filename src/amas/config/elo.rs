use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

pub(crate) fn default_word_k_factor_ratio() -> f64 {
    0.5
}

pub(crate) fn default_min_elo() -> f64 {
    400.0
}
pub(crate) fn default_max_elo() -> f64 {
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IgeConfig {
    pub batch_size: u32,
    pub interval_scale: f64,
    pub ucb_confidence_coeff: f64,
    pub default_confidence: f64,
    #[serde(default = "default_difficulty_bin_count")]
    pub difficulty_bin_count: usize,
    #[serde(default = "default_ratio_bin_count")]
    pub ratio_bin_count: usize,
    #[serde(default)]
    pub pretrained_difficulty_rewards: Option<Vec<f64>>,
    #[serde(default)]
    pub pretrained_ratio_rewards: Option<Vec<f64>>,
}

pub(crate) fn default_difficulty_bin_count() -> usize {
    20
}
pub(crate) fn default_ratio_bin_count() -> usize {
    16
}

impl Default for IgeConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            interval_scale: 1.0,
            ucb_confidence_coeff: 2.0,
            default_confidence: 0.6,
            difficulty_bin_count: 20,
            ratio_bin_count: 16,
            pretrained_difficulty_rewards: None,
            pretrained_ratio_rewards: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SwdConfig {
    pub max_history_size: usize,
    pub history_filter_threshold: f64,
    pub fallback_confidence: f64,
    #[serde(default = "default_similarity_cache_ttl_secs")]
    pub similarity_cache_ttl_secs: u64,
}

pub(crate) fn default_similarity_cache_ttl_secs() -> u64 {
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
