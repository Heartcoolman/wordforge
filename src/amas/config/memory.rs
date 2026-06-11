use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryModelConfig {
    pub short_term_learning_rate: f64,
    pub medium_term_learning_rate: f64,
    pub long_term_learning_rate: f64,
    pub composite_weight_short: f64,
    pub composite_weight_medium: f64,
    pub composite_weight_long: f64,
    pub consolidation_rate_scale: f64,
    pub consolidation_bonus: f64,
    pub mastery_composite_threshold: f64,
    pub mastery_accuracy_threshold: f64,
    pub mastery_streak_threshold: u32,
    pub reviewing_threshold: f64,
    pub half_life_base_epsilon: f64,
    pub half_life_time_unit_secs: f64,
    #[serde(default = "default_half_life_power")]
    pub half_life_power: f64,
    pub recall_risk_bonus: f64,
    pub recall_risk_threshold: f64,
    #[serde(default = "default_base_desired_retention")]
    pub base_desired_retention: f64,
    #[serde(default = "default_passive_decay_half_life_days")]
    pub passive_decay_half_life_days: f64,
    #[serde(default = "default_passive_decay_power")]
    pub passive_decay_power: f64,
    #[serde(default = "default_mastery_window_size")]
    pub mastery_window_size: u32,
    #[serde(default = "default_streak_min_gap_ms")]
    pub streak_min_gap_ms: i64,
    #[serde(default = "default_stability_base_days")]
    pub stability_base_days: f64,
    /// DEPRECATED（FSRS-6 起）：遗忘曲线 factor 由 `w[20]` 派生（`curve_factor()`），
    /// 本字段仅为旧配置/DB 快照反序列化兼容保留，运行时不再读取。
    #[serde(default = "default_forgetting_curve_factor")]
    pub forgetting_curve_factor: f64,
    /// DEPRECATED（FSRS-6 起）：遗忘曲线 decay 即 trainable 参数 `w[20]`（`curve_decay()`），
    /// 本字段仅为旧配置/DB 快照反序列化兼容保留，运行时不再读取。
    #[serde(default = "default_forgetting_curve_decay")]
    pub forgetting_curve_decay: f64,
    /// 2021 MaiMemo study: forgetting curve has non-zero asymptote R→floor (not 0)
    /// FSRS-6 标准曲线无渐近线，默认 0.0；保留为可调项。
    #[serde(default = "default_forgetting_curve_floor")]
    pub forgetting_curve_floor: f64,

    // === FSRS-6 DSR parameters (21 weights; w[19]=同日饱和指数, w[20]=曲线 decay) ===
    /// 反序列化兼容 19 维 FSRS-5 旧配置：自动迁移（w19=0 维持旧同日公式，w20=0.5 维持旧 decay）。
    #[serde(default = "default_w", deserialize_with = "de_w_legacy_or_fsrs6")]
    pub w: [f64; 21],
    // === 原 mastery.rs 模块级常量 ===
    #[serde(default = "default_alpha_scale")]
    pub alpha_scale: f64,
    #[serde(default = "default_alpha_min")]
    pub alpha_min: f64,
    #[serde(default = "default_alpha_max")]
    pub alpha_max: f64,
    /// 证据递减阻尼时间常数：成功复习时 alpha_eff(n)=1-(1-alpha)·e^{-(n-1)/tau}；
    /// 0.0=关闭（冻结语义），DB 旧快照/未声明配置反序列化即得旧行为。
    #[serde(default = "default_alpha_ramp_tau")]
    pub alpha_ramp_tau: f64,
    #[serde(default = "default_forgetting_threshold")]
    pub forgetting_threshold: f64,
    // === 原 mdm.rs 模块级常量 ===
    #[serde(default = "default_retention_min")]
    pub retention_min: f64,
    #[serde(default = "default_retention_max")]
    pub retention_max: f64,
    #[serde(default = "default_max_interval_days")]
    pub max_interval_days: f64,
    #[serde(default = "default_min_interval_secs")]
    pub min_interval_secs: i64,
    #[serde(default = "default_high_accuracy_threshold")]
    pub high_accuracy_threshold: f64,
    #[serde(default = "default_high_accuracy_retention_boost")]
    pub high_accuracy_retention_boost: f64,
    #[serde(default = "default_high_fatigue_threshold")]
    pub high_fatigue_threshold: f64,
    #[serde(default = "default_high_fatigue_retention_drop")]
    pub high_fatigue_retention_drop: f64,
    #[serde(default = "default_low_motivation_threshold")]
    pub low_motivation_threshold: f64,
    #[serde(default = "default_low_motivation_retention_drop")]
    pub low_motivation_retention_drop: f64,
}

pub(crate) fn default_base_desired_retention() -> f64 {
    0.85
}
pub(crate) fn default_half_life_power() -> f64 {
    1.5
}
pub(crate) fn default_passive_decay_half_life_days() -> f64 {
    30.0
}
pub(crate) fn default_passive_decay_power() -> f64 {
    0.5
}
pub(crate) fn default_mastery_window_size() -> u32 {
    20
}
pub(crate) fn default_streak_min_gap_ms() -> i64 {
    1_800_000
}
pub(crate) fn default_stability_base_days() -> f64 {
    20.0
}
pub(crate) fn default_forgetting_curve_factor() -> f64 {
    19.0 / 81.0
}
pub(crate) fn default_forgetting_curve_decay() -> f64 {
    -0.5
}
pub(crate) fn default_forgetting_curve_floor() -> f64 {
    0.0
}

/// 19 维 FSRS-5 旧配置迁移用：旧同日复习公式无 S^(-w19) 饱和项，置 0 保持行为。
const LEGACY_W19_SAME_DAY_SATURATION: f64 = 0.0;
/// 19 维 FSRS-5 旧配置迁移用：旧曲线固定 decay |−0.5|，保持曲线形状连续。
const LEGACY_W20_DECAY: f64 = 0.5;

/// `w` 反序列化：21 维（FSRS-6）直取；19 维（FSRS-5 旧配置/DB 快照）自动迁移。
fn de_w_legacy_or_fsrs6<'de, D>(deserializer: D) -> Result<[f64; 21], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Vec<f64> = Vec::deserialize(deserializer)?;
    match v.len() {
        21 => Ok(std::array::from_fn(|i| v[i])),
        19 => {
            let mut w = [0.0; 21];
            w[..19].copy_from_slice(&v);
            w[19] = LEGACY_W19_SAME_DAY_SATURATION;
            w[20] = LEGACY_W20_DECAY;
            Ok(w)
        }
        n => Err(serde::de::Error::custom(format!(
            "memoryModel.w expects 21 weights (FSRS-6) or legacy 19 (FSRS-5), got {n}"
        ))),
    }
}

pub(crate) fn default_alpha_scale() -> f64 {
    0.3
}
pub(crate) fn default_alpha_min() -> f64 {
    0.1
}
pub(crate) fn default_alpha_max() -> f64 {
    0.5
}
pub(crate) fn default_alpha_ramp_tau() -> f64 {
    // 默认关闭：未声明该旋钮的配置（含 DB 历史快照）必须反序列化为精确旧语义
    0.0
}
pub(crate) fn default_forgetting_threshold() -> f64 {
    0.2
}
pub(crate) fn default_retention_min() -> f64 {
    // bench tuned v3：抬高下限防过度拉长间隔
    0.75
}
pub(crate) fn default_retention_max() -> f64 {
    0.95
}
pub(crate) fn default_max_interval_days() -> f64 {
    90.0
}
pub(crate) fn default_min_interval_secs() -> i64 {
    60
}
pub(crate) fn default_high_accuracy_threshold() -> f64 {
    0.9
}
pub(crate) fn default_high_accuracy_retention_boost() -> f64 {
    // bench tuned v3
    0.03
}
pub(crate) fn default_high_fatigue_threshold() -> f64 {
    0.6
}
pub(crate) fn default_high_fatigue_retention_drop() -> f64 {
    0.05
}
pub(crate) fn default_low_motivation_threshold() -> f64 {
    -0.2
}
pub(crate) fn default_low_motivation_retention_drop() -> f64 {
    0.03
}

// FSRS-6 公版默认参数（py-fsrs DEFAULT_PARAMETERS）—— 与前端 schema memoryModel.w[*] default 字面对齐
// 产线由 amas_config.toml 的 [memoryModel].w 覆盖；该 default 仅在
// 无配置文件场景下生效，并对应前端"重置默认"按钮的目标值。
pub(crate) fn default_w() -> [f64; 21] {
    [
        0.212,  // w0: initial stability after Again（FSRS-6 公版）
        1.2931, // w1: initial stability after Hard
        2.3065, // w2: initial stability after Good
        8.2956, // w3: initial stability after Easy
        6.4133, // w4: initial difficulty base
        0.8334, // w5: difficulty scaling
        3.0194, // w6: difficulty change per grade
        0.001,  // w7: mean reversion weight
        1.8722, // w8: stability increase base
        0.1666, // w9: stability increase power
        0.796,  // w10: spacing effect
        1.4835, // w11: post-lapse stability base
        0.0614, // w12: post-lapse difficulty power
        0.2629, // w13: post-lapse stability power
        1.6483, // w14: post-lapse R scaling
        0.6014, // w15: Hard penalty
        1.8729, // w16: Easy bonus
        0.5425, // w17: same-day review scaling
        0.0912, // w18: same-day review offset
        0.0658, // w19: same-day stability saturation（FSRS-6 新增）
        0.1542, // w20: forgetting curve decay（FSRS-6 新增，trainable）
    ]
}

impl MemoryModelConfig {
    /// FSRS-6 遗忘曲线 decay = `w[20]`（trainable），钳到安全域防 0 除/爆幂。
    pub fn curve_decay(&self) -> f64 {
        self.w[20].clamp(0.05, 2.0)
    }

    /// FSRS-6 遗忘曲线 factor = 0.9^(-1/decay) − 1，保证 R(S,S)=0.9（floor=0 时）。
    pub fn curve_factor(&self) -> f64 {
        0.9_f64.powf(-1.0 / self.curve_decay()) - 1.0
    }
}

impl Default for MemoryModelConfig {
    fn default() -> Self {
        Self {
            short_term_learning_rate: 0.85,
            medium_term_learning_rate: 0.30,
            long_term_learning_rate: 0.12,
            composite_weight_short: 0.20,
            composite_weight_medium: 0.30,
            composite_weight_long: 0.50,
            consolidation_rate_scale: 0.25,
            consolidation_bonus: 1.5,
            mastery_composite_threshold: 0.30,
            mastery_accuracy_threshold: 0.65,
            mastery_streak_threshold: 1,
            reviewing_threshold: 0.4,
            half_life_base_epsilon: 0.3,
            half_life_time_unit_secs: 1296000.0,
            half_life_power: 1.5,
            recall_risk_bonus: 0.2,
            recall_risk_threshold: 0.55,
            // FSRS 官方推荐 desired_retention，与 R(S,S)=0.9 语义自洽
            base_desired_retention: 0.9,
            passive_decay_half_life_days: 30.0,
            passive_decay_power: 0.30,
            mastery_window_size: 20,
            streak_min_gap_ms: 1_800_000,
            stability_base_days: 20.0,
            // DEPRECATED 字段：运行时曲线参数从 w[20] 派生（curve_decay/curve_factor）
            forgetting_curve_factor: 19.0 / 81.0,
            forgetting_curve_decay: -0.5,
            // FSRS-6 标准曲线无渐近线；保留为可调项
            forgetting_curve_floor: 0.0,
            w: default_w(),
            alpha_scale: 0.3,
            alpha_min: 0.1,
            alpha_max: 0.5,
            alpha_ramp_tau: 0.0,
            forgetting_threshold: 0.2,
            retention_min: 0.75,
            retention_max: 0.95,
            max_interval_days: 90.0,
            min_interval_secs: 60,
            high_accuracy_threshold: 0.9,
            high_accuracy_retention_boost: 0.03,
            high_fatigue_threshold: 0.6,
            high_fatigue_retention_drop: 0.05,
            low_motivation_threshold: -0.2,
            low_motivation_retention_drop: 0.03,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvmConfig {
    #[serde(default = "default_evm_diversity_log_divisor")]
    pub diversity_log_divisor: f64,
    #[serde(default = "default_evm_diversity_bonus_cap")]
    pub diversity_bonus_cap: f64,
    #[serde(default = "default_evm_diversity_growth_rate")]
    pub diversity_growth_rate: f64,
}

pub(crate) fn default_evm_diversity_log_divisor() -> f64 {
    5.0
}
pub(crate) fn default_evm_diversity_bonus_cap() -> f64 {
    0.3
}
pub(crate) fn default_evm_diversity_growth_rate() -> f64 {
    0.2
}

impl Default for EvmConfig {
    fn default() -> Self {
        Self {
            diversity_log_divisor: 5.0,
            diversity_bonus_cap: 0.3,
            diversity_growth_rate: 0.2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IadConfig {
    pub interference_penalty_factor: f64,
    pub interference_penalty_cap: f64,
    pub max_confusion_pairs: usize,
    pub new_confusion_initial_score: f64,
    pub confusion_update_increment: f64,
    pub interval_shortening_factor: f64,
    #[serde(default = "default_confusion_decay_rate")]
    pub confusion_decay_rate: f64,
}

pub(crate) fn default_confusion_decay_rate() -> f64 {
    0.05
}

impl Default for IadConfig {
    fn default() -> Self {
        Self {
            interference_penalty_factor: 0.1,
            interference_penalty_cap: 0.3,
            max_confusion_pairs: 20,
            new_confusion_initial_score: 0.2,
            confusion_update_increment: 0.2,
            interval_shortening_factor: 0.5,
            confusion_decay_rate: 0.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MtpConfig {
    pub morpheme_transfer_coeff: f64,
    pub morpheme_bonus_cap: f64,
    pub known_morpheme_decay: f64,
    pub new_morpheme_initial_coeff: f64,
    pub max_known_morphemes: usize,
}

impl Default for MtpConfig {
    fn default() -> Self {
        Self {
            morpheme_transfer_coeff: 0.15,
            morpheme_bonus_cap: 0.3,
            known_morpheme_decay: 0.9,
            new_morpheme_initial_coeff: 0.5,
            max_known_morphemes: 500,
        }
    }
}
