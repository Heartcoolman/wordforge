use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    pub ensemble_enabled: bool,
    pub heuristic_enabled: bool,
    pub ige_enabled: bool,
    pub swd_enabled: bool,
    pub mdm_enabled: bool,
    /// SSP-MMC: 最优间隔调度（离线 DP 预计算策略表）
    #[serde(default)]
    pub ssp_enabled: bool,
    /// ② 混淆隔离排程（Phase 1b）：开启后选词调用方将已出现词的高分混淆对端注入
    /// SessionSelectionContext.confusion_exclude_word_ids（评分乘 confusion_isolation_dampen）。
    /// 默认 false → 调用方不填充 → 选词 bit-exact legacy。
    #[serde(default)]
    pub confusion_isolation_enabled: bool,
    /// ③ 跨题型多痕迹（Phase 2）：开启后 mastery 状态按 question_mode 分痕迹
    /// （`mastery:{word}:{mode}`），选词按 min-recall 跨痕迹聚合。默认 false → 单一
    /// `mastery:{word}` 键、bit-exact legacy。
    #[serde(default)]
    pub multi_trace_enabled: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            ensemble_enabled: true,
            heuristic_enabled: true,
            ige_enabled: true,
            swd_enabled: true,
            mdm_enabled: true,
            ssp_enabled: false,
            confusion_isolation_enabled: false,
            multi_trace_enabled: false,
        }
    }
}
