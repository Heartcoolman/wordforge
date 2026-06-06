use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod elo;
mod ensemble;
mod feature_flags;
mod learning;
mod memory;
mod modeling;
mod ssp;
mod validation;
mod word_selector;

pub use elo::*;
pub use ensemble::*;
pub use feature_flags::*;
pub use learning::*;
pub use memory::*;
pub use modeling::*;
pub use ssp::*;
pub use word_selector::*;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(rename_all = "camelCase")]
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
    #[serde(default)]
    pub ssp: SspConfig,
    #[serde(default)]
    pub evm: EvmConfig,
}

impl AMASConfig {
    pub fn from_env(env_config: &crate::config::AMASEnvConfig) -> Self {
        let mut config = Self::default();
        config.feature_flags.ensemble_enabled = env_config.ensemble_enabled;
        config.monitoring.sample_rate = env_config.monitor_sample_rate;
        config
    }

    pub fn load_from_toml(path: &str) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("读取配置文件失败: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("解析 TOML 配置失败: {e}"))
    }

    pub fn write_to_toml(&self, path: &str) -> Result<(), String> {
        let content =
            toml::to_string_pretty(self).map_err(|e| format!("序列化 TOML 配置失败: {e}"))?;
        // 先写临时文件再原子 rename，避免 watcher 读到不完整内容
        let tmp = format!("{path}.tmp");
        std::fs::write(&tmp, &content).map_err(|e| format!("写入临时配置文件失败: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("原子重命名配置文件失败: {e}"))
    }
}

#[cfg(test)]
mod tests;
