use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelingConfig {
    pub attention_smoothing: f64,
    pub confidence_decay: f64,
    pub min_confidence: f64,
    pub fatigue_increase_rate: f64,
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

pub(crate) fn default_response_speed_max_ms() -> f64 {
    10000.0
}
pub(crate) fn default_fatigue_quit_increase() -> f64 {
    0.2
}
pub(crate) fn default_engagement_pause_penalty() -> f64 {
    0.05
}
pub(crate) fn default_engagement_pause_penalty_max() -> f64 {
    0.3
}
pub(crate) fn default_engagement_switch_penalty() -> f64 {
    0.1
}
pub(crate) fn default_engagement_switch_penalty_max() -> f64 {
    0.3
}
pub(crate) fn default_engagement_focus_loss_base_ms() -> f64 {
    30000.0
}
pub(crate) fn default_engagement_focus_loss_penalty_max() -> f64 {
    0.3
}
pub(crate) fn default_cognitive_profile_alpha() -> f64 {
    0.1
}
pub(crate) fn default_trend_alpha() -> f64 {
    0.05
}

impl Default for ModelingConfig {
    fn default() -> Self {
        Self {
            attention_smoothing: 0.30,
            confidence_decay: 0.99,
            min_confidence: 0.10,
            fatigue_increase_rate: 0.02,
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
