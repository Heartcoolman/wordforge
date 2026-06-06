use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

pub(crate) fn default_warmup_heuristic_boost() -> f64 {
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
