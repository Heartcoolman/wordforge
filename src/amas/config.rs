use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    pub ensemble_enabled: bool,
    pub heuristic_enabled: bool,
    pub ige_enabled: bool,
    pub swd_enabled: bool,
    pub mdm_enabled: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            ensemble_enabled: true,
            heuristic_enabled: true,
            ige_enabled: true,
            swd_enabled: true,
            mdm_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleConfig {
    pub base_weight_heuristic: f64,
    pub base_weight_ige: f64,
    pub base_weight_swd: f64,
    pub warmup_samples: u64,
    pub blend_scale: f64,
    pub blend_max: f64,
    pub min_weight: f64,
}

impl Default for EnsembleConfig {
    fn default() -> Self {
        Self {
            base_weight_heuristic: 0.20,
            base_weight_ige: 0.40,
            base_weight_swd: 0.40,
            warmup_samples: 20,
            blend_scale: 100.0,
            blend_max: 0.50,
            min_weight: 0.15,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelingConfig {
    pub attention_smoothing: f64,
    pub confidence_decay: f64,
    pub min_confidence: f64,
    pub fatigue_increase_rate: f64,
    pub fatigue_recovery_rate: f64,
    pub motivation_momentum: f64,
}

impl Default for ModelingConfig {
    fn default() -> Self {
        Self {
            attention_smoothing: 0.30,
            confidence_decay: 0.99,
            min_confidence: 0.10,
            fatigue_increase_rate: 0.02,
            fatigue_recovery_rate: 0.001,
            motivation_momentum: 0.1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintConfig {
    pub high_fatigue_threshold: f64,
    pub low_attention_threshold: f64,
    pub low_motivation_threshold: f64,
    pub max_batch_size_when_fatigued: u32,
    pub max_new_ratio_when_fatigued: f64,
    pub max_difficulty_when_fatigued: f64,
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AMASConfig {
    pub feature_flags: FeatureFlags,
    pub ensemble: EnsembleConfig,
    pub modeling: ModelingConfig,
    pub constraints: ConstraintConfig,
    pub monitoring: MonitoringConfig,
    pub cold_start: ColdStartConfig,
    pub objective_weights: ObjectiveWeights,
}

impl Default for AMASConfig {
    fn default() -> Self {
        Self {
            feature_flags: FeatureFlags::default(),
            ensemble: EnsembleConfig::default(),
            modeling: ModelingConfig::default(),
            constraints: ConstraintConfig::default(),
            monitoring: MonitoringConfig::default(),
            cold_start: ColdStartConfig::default(),
            objective_weights: ObjectiveWeights::default(),
        }
    }
}

impl AMASConfig {
    pub fn from_env(env_config: &crate::config::AMASEnvConfig) -> Self {
        let mut config = Self::default();
        config.feature_flags.ensemble_enabled = env_config.ensemble_enabled;
        config.monitoring.sample_rate = env_config.monitor_sample_rate;
        config
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.monitoring.sample_rate) {
            return Err("monitoring.sample_rate must be in [0,1]".to_string());
        }

        if !(0.0..=1.0).contains(&self.constraints.high_fatigue_threshold)
            || !(0.0..=1.0).contains(&self.constraints.low_attention_threshold)
            || !(-1.0..=1.0).contains(&self.constraints.low_motivation_threshold)
        {
            return Err("invalid constraint thresholds".to_string());
        }

        if self.ensemble.base_weight_heuristic <= 0.0
            || self.ensemble.base_weight_ige <= 0.0
            || self.ensemble.base_weight_swd <= 0.0
        {
            return Err("ensemble base weights must be > 0".to_string());
        }

        if self.ensemble.min_weight <= 0.0 || self.ensemble.min_weight > 1.0 {
            return Err("ensemble.min_weight must be in (0,1]".to_string());
        }

        if 3.0 * self.ensemble.min_weight > 1.0 {
            return Err("ensemble.min_weight too large: 3 * min_weight must be <= 1.0".to_string());
        }

        if self.objective_weights.retention < 0.0
            || self.objective_weights.accuracy < 0.0
            || self.objective_weights.speed < 0.0
            || self.objective_weights.fatigue < 0.0
            || self.objective_weights.frustration < 0.0
        {
            return Err("objective_weights must be >= 0".to_string());
        }

        let sum = self.objective_weights.retention
            + self.objective_weights.accuracy
            + self.objective_weights.speed
            + self.objective_weights.fatigue
            + self.objective_weights.frustration;
        if sum <= 0.0 {
            return Err("objective_weights sum must be > 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = AMASConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn invalid_config_is_rejected() {
        let mut cfg = AMASConfig::default();
        cfg.monitoring.sample_rate = 2.0;
        assert!(cfg.validate().is_err());
    }
}
