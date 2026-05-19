use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
    /// SSP-MMC: 最优间隔调度（离线 DP 预计算策略表）
    #[serde(default)]
    pub ssp_enabled: bool,
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
            ssp_enabled: false,
        }
    }
}
