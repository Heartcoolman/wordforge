use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    // ── T1.2 动态 K（趋势自适应）。默认 off → bit-exact 退回固定 K。 ──
    /// 开关：true 时按近期残差趋势动态调 K。
    #[serde(default)]
    pub k_dynamic_enabled: bool,
    /// 残差趋势 EWMA 权重 ∈ (0,1]：越大越看重最近一次残差。
    #[serde(default = "default_k_trend_weight")]
    pub k_trend_weight: f64,
    /// 趋势增益：|趋势| 每单位放大 K 的系数（连续同向误差 → 增 K 追漂移）。
    #[serde(default = "default_k_trend_gain")]
    pub k_trend_gain: f64,
    /// 稳态阻尼：残差震荡(|趋势|→0)时把 K 乘子下压的量（降噪）。
    #[serde(default = "default_k_trend_damp")]
    pub k_trend_damp: f64,
    /// K 乘子下界（防过度降 K 致停滞）。
    #[serde(default = "default_k_min_factor")]
    pub k_min_factor: f64,
    /// K 乘子上界（防放大噪声）。
    #[serde(default = "default_k_max_factor")]
    pub k_max_factor: f64,
    // ── T1.1 Parallel Elo（双链解耦）。默认 off → 选词读估计链(rating)，bit-exact 退回单链。 ──
    /// 开关：true 时 ZPD 选词读「选词链」(rating_select，延迟快照)，更新写「估计链」(rating)，
    /// 消除「选择依赖被估计量」的耦合偏差。难度消费者(difflogit/analytics)恒读估计链。
    #[serde(default)]
    pub parallel_elo_enabled: bool,
    /// 选词链延迟刷新间隔（按该词全局对局数）：每 N 局把选词链快照到估计链当前值。
    #[serde(default = "default_parallel_elo_refresh_games")]
    pub parallel_elo_refresh_games: u32,
}

pub(crate) fn default_parallel_elo_refresh_games() -> u32 {
    8
}

pub(crate) fn default_word_k_factor_ratio() -> f64 {
    0.5
}
pub(crate) fn default_k_trend_weight() -> f64 {
    0.3
}
pub(crate) fn default_k_trend_gain() -> f64 {
    1.0
}
pub(crate) fn default_k_trend_damp() -> f64 {
    0.2
}
pub(crate) fn default_k_min_factor() -> f64 {
    0.5
}
pub(crate) fn default_k_max_factor() -> f64 {
    2.0
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
            k_dynamic_enabled: false,
            k_trend_weight: 0.3,
            k_trend_gain: 1.0,
            k_trend_damp: 0.2,
            k_min_factor: 0.5,
            k_max_factor: 2.0,
            parallel_elo_enabled: false,
            parallel_elo_refresh_games: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
            // bench tuned v3：decay 更慢（500 事件尺度）、上限收紧
            confidence_decay_cap: 0.3,
            confidence_min: 0.2,
            confidence_decay_scale: 500.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
            // bench tuned v3
            default_confidence: 0.65,
            difficulty_bin_count: 20,
            ratio_bin_count: 16,
            pretrained_difficulty_rewards: None,
            pretrained_ratio_rewards: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
