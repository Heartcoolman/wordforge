use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SspCostParams {
    /// stability 对 recall cost 的线性调制系数
    #[serde(default)]
    pub recall_s_coeff: f64,
    /// difficulty 对 recall cost 的线性调制系数
    #[serde(default)]
    pub recall_d_coeff: f64,
    /// stability 对 forget cost 的线性调制系数
    #[serde(default)]
    pub forget_s_coeff: f64,
    /// difficulty 对 forget cost 的线性调制系数
    #[serde(default)]
    pub forget_d_coeff: f64,
}

impl Default for SspCostParams {
    fn default() -> Self {
        Self {
            recall_s_coeff: 0.0,
            recall_d_coeff: 0.0,
            forget_s_coeff: 0.0,
            forget_d_coeff: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SspConfig {
    pub recall_cost: f64,
    pub forget_cost: f64,
    /// 目标 stability（天）：halflife 达到此值视为"长期记住"
    pub target_stability_days: f64,
    /// 离散化底数（与墨墨一致）
    pub base: f64,
    /// stability log 等比 bins 的最小指数
    pub min_index: i32,
    /// stability log 等比 bins 的最大指数
    pub max_index: i32,
    /// 遗忘后难度增量（映射到 D ∈ [1,10]）
    pub difficulty_offset_on_lapse: f64,
    /// DP 最大迭代次数
    pub max_iterations: u32,
    /// DP 收敛阈值
    pub convergence_threshold: f64,
    /// Bellman 折扣因子 γ ∈ [0,1]，防止无穷代价堆积
    #[serde(default = "default_ssp_discount_factor")]
    pub discount_factor: f64,
    /// 双网格离散化：小 S 用 log 间距，大 S 用线性间距
    #[serde(default)]
    pub dual_grid_enabled: bool,
    /// 双网格切换阈值（天）
    #[serde(default = "default_ssp_dual_grid_threshold")]
    pub dual_grid_threshold_days: f64,
    /// 线性间距步长（天）
    #[serde(default = "default_ssp_linear_step")]
    pub linear_step_days: f64,
    /// 参数化代价函数：recall/forget cost 随 S/D 调制
    #[serde(default)]
    pub cost_params: SspCostParams,
    /// 3D 求解器：目标保持率搜索下界
    #[serde(default = "default_ssp_r_min")]
    pub r_min: f64,
    /// 3D 求解器：目标保持率搜索上界
    #[serde(default = "default_ssp_r_max")]
    pub r_max: f64,
    /// 3D 求解器：保持率离散步长
    #[serde(default = "default_ssp_r_step")]
    pub r_step: f64,
    /// 评分概率分布 [Hard, Good, Easy]
    #[serde(default = "default_ssp_rating_probs")]
    pub review_rating_probs: [f64; 3],
}

pub(crate) fn default_ssp_discount_factor() -> f64 {
    1.0
}
pub(crate) fn default_ssp_r_min() -> f64 {
    0.70
}
pub(crate) fn default_ssp_r_max() -> f64 {
    0.99
}
pub(crate) fn default_ssp_r_step() -> f64 {
    0.01
}
pub(crate) fn default_ssp_rating_probs() -> [f64; 3] {
    [0.224, 0.631, 0.145]
}
pub(crate) fn default_ssp_dual_grid_threshold() -> f64 {
    10.0
}
pub(crate) fn default_ssp_linear_step() -> f64 {
    5.0
}

impl Default for SspConfig {
    fn default() -> Self {
        Self {
            recall_cost: 3.0,
            forget_cost: 9.0,
            target_stability_days: 360.0,
            base: 1.05,
            min_index: -30,
            max_index: 122,
            difficulty_offset_on_lapse: 1.0,
            max_iterations: 200_000,
            convergence_threshold: 0.1,
            discount_factor: 1.0,
            dual_grid_enabled: true,
            dual_grid_threshold_days: 10.0,
            linear_step_days: 5.0,
            cost_params: SspCostParams::default(),
            r_min: 0.70,
            r_max: 0.99,
            r_step: 0.01,
            review_rating_probs: [0.224, 0.631, 0.145],
        }
    }
}
