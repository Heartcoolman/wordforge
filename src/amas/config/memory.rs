use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MemoryModelConfig {
    pub mastery_composite_threshold: f64,
    pub mastery_accuracy_threshold: f64,
    pub mastery_streak_threshold: u32,
    pub reviewing_threshold: f64,
    pub half_life_base_epsilon: f64,
    pub recall_risk_bonus: f64,
    pub recall_risk_threshold: f64,
    #[serde(default = "default_base_desired_retention")]
    pub base_desired_retention: f64,
    #[serde(default = "default_mastery_window_size")]
    pub mastery_window_size: u32,
    #[serde(default = "default_streak_min_gap_ms")]
    pub streak_min_gap_ms: i64,
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
    /// 双腿信任调度·成功腿时间常数 τ_s：成功复习（grade≥2）时
    /// alpha_eff(k)=1-(1-alpha)·e^{-(k-1)/tau}，k=本次记账后的 correct_streak
    /// （失败清零→阻尼重启）；0.0=关闭（冻结语义），DB 旧快照/未声明配置反序列化即得旧行为。
    #[serde(default = "default_alpha_ramp_tau")]
    pub alpha_ramp_tau: f64,
    /// 双腿信任调度·失败腿时间常数 τ_f：失败（Again）时
    /// alpha_eff(f)=1-(1-alpha)·e^{-(f-1)/tau}，f=含本次的累计 lapse 数
    /// （首错 f=1 即 no-op = 偶发失误保护）；0.0=关闭（冻结语义）。
    #[serde(default = "default_alpha_lapse_ramp_tau")]
    pub alpha_lapse_ramp_tau: f64,
    #[serde(default = "default_forgetting_threshold")]
    pub forgetting_threshold: f64,
    // === GSP 调度策略头（Graduated Scheduling Policy；契约 benchmarks/maimemo/GSP_SPEC.md）===
    // 全部默认 = 关闭 = 与不带该键的旧配置/DB 快照逐位等价（bit-exact legacy）。
    /// 二元成功复习映射的 FSRS grade：3=Good（旧默认，首评 S0=w[2]，bit-exact legacy）；
    /// 4=Easy（首评 S0=w[3] + 成功复习带 w[16] easy_bonus + 进入 FSRS-6 faithful 状态路径，
    /// 见 mdm.rs §3.5）。FSRS-6 公版 21 维 w 与 grade=4 共拟合 → 候选用 4 恢复预测腿校准。
    /// 入口 clamp：非 {3,4} 一律夹回 3。失败恒映射 1=Again。
    #[serde(default = "default_gsp_success_grade")]
    pub gsp_success_grade: u32,
    /// 调度区间硬帽（天）。0=关闭。作用于 interval_scale 之后，与既有 90 天硬帽复合 min(90, cap)。
    #[serde(default = "default_gsp_interval_cap_days")]
    pub gsp_interval_cap_days: f64,
    /// 毕业连击阈值 k。0=关闭。当词的 correct_streak >= k 时，调度区间获得 floor 下限。
    #[serde(default = "default_gsp_graduation_streak")]
    pub gsp_graduation_streak: u32,
    /// 毕业下限（天）。在 scale 之后、cap 之前施加 max(interval, floor)（随 streak 关闭而失效）。
    #[serde(default = "default_gsp_graduation_floor_days")]
    pub gsp_graduation_floor_days: f64,
    /// 年轻卡（stability < band）目标保持率（随 band 关闭而失效）。
    #[serde(default = "default_gsp_young_retention")]
    pub gsp_young_retention: f64,
    /// 成熟卡（stability >= band）目标保持率（随 band 关闭而失效）。
    #[serde(default = "default_gsp_mature_retention")]
    pub gsp_mature_retention: f64,
    /// 成熟度分带阈值（天，按 stability）。0=关闭。>0 时 interval 求解的目标保持率由
    /// young/mature 替换自适应 desired_retention（仅区间求解口径，预测/recall 路径不受影响）。
    #[serde(default = "default_gsp_maturity_band_days")]
    pub gsp_maturity_band_days: f64,
    /// 复习负载平滑：确定性区间抖动幅度。0=关闭。>0 时在 cap 之后施加 days·(1+fuzz·u)，
    /// u∈[-1,1) 由 stability/review_count 派生（见 GSP_SPEC §7），错峰削平同步复习波。
    #[serde(default = "default_gsp_interval_fuzz")]
    pub gsp_interval_fuzz: f64,
    // === GSP 退役（retire）：毕业后过学词冻结到长间隔（2026-07-08 真半衰期口径战役）===
    /// 退役所需最小复习次数。0=关闭（bit-exact legacy）。>0 时：毕业（graduated）且
    /// review_count/stability/correct_streak 三门槛同时满足 → 调度区间直接取 retire 间隔
    /// （在 cap/fuzz 之前 return，可越过区间帽）。注意 review_count 含全部历史，warm 词
    /// 低门槛会首评即退役 —— 应与 `gsp_retire_min_streak` 搭配使用（契约 GSP_SPEC §9）。
    #[serde(default = "default_gsp_retire_after_reviews")]
    pub gsp_retire_after_reviews: u32,
    /// 退役间隔（天）。默认 365。
    #[serde(default = "default_gsp_retire_interval_days")]
    pub gsp_retire_interval_days: f64,
    /// 退役 stability 门槛（天）。默认 0=不设。
    #[serde(default = "default_gsp_retire_min_stability")]
    pub gsp_retire_min_stability: f64,
    /// 退役连击门槛。默认 0=不设。连击是「近期无失败」的直接证据，是记忆已固化的
    /// 强代理（离线 oracle 校准：脏历史词 streak≥4 时估计半衰期稳越 30 天）。
    #[serde(default = "default_gsp_retire_min_streak")]
    pub gsp_retire_min_streak: u32,
    // === per-word difficulty logit 加性项（v6 预测层；契约见 benchmarks/maimemo pred_*.py）===
    /// 预测/调度 recall **读出**在 logit 域加 `β·(REF − word_difficulty[1,10])`：难词降 p、易词升 p，
    /// 补 FSRS 二元映射下「预测随难度扁平」的区分度/校准残差（maimemo TEST: AUC+0.015 反超 dhp、
    /// logLoss−0.005）。仅作用于 `recall_probability_predicted` 读出路径，**不入内部 S/D 更新**
    /// （update_strength 的 r 仍用纯 recall_probability）。0.0=关闭（bit-exact legacy）；
    /// 无每词难度的调用点传 None → 自然 no-op。
    #[serde(default = "default_difficulty_logit_weight")]
    pub difficulty_logit_weight: f64,
    /// difficulty logit 项参考点（[1,10] 标度）：word_difficulty==REF 时位移为 0。默认 5.0。
    #[serde(default = "default_difficulty_logit_ref")]
    pub difficulty_logit_ref: f64,
    // === 预测读出层 logit 重校准 + 复习次数残差（v7；difficulty_logit 的推广）===
    /// 读出 recall 在 logit 域施加：`logit(p) = base_scale·logit(p_fsrs) + intercept
    /// + β_diff·(REF−D) + β_nrev·ln(1+review_count)`。前两项是 Platt/温度重校准（FSRS-6 在真实
    /// 词汇数据上略过自信），β_nrev 补复习次数残差（stability 未完全吸收）。系数**按部署离线拟合**
    /// （benchmarks/maimemo/pred_calib.py，maimemo held-out: AUC+0.0116/logLoss−0.0084/ECE 0.022→0.016，
    /// 5/5 seed），**不跨域迁移**（synthetic 退化，须 prod_replay 复核）。仅作用 `recall_probability_predicted`
    /// 读出路径，不入 S/D 更新。默认 base_scale=1/intercept=0/β_nrev=0 → 退化为纯 difficulty_logit。
    #[serde(default = "default_pred_logit_base_scale")]
    pub pred_logit_base_scale: f64,
    #[serde(default = "default_pred_logit_intercept")]
    pub pred_logit_intercept: f64,
    #[serde(default = "default_pred_logit_review_count_weight")]
    pub pred_logit_review_count_weight: f64,
    // === 冷启动难度先验（Phase 1a；仅首评 review_count==0 调整 S₀/D₀，之后交还 FSRS）===
    // 全部权重默认 0.0 → deltas=(0,0) → 与不带该键的旧配置/DB 快照逐位等价（bit-exact legacy）。
    // 公式见 mdm.rs ColdStartPriors::deltas / dhp_reference.py 镜像。ref/scale 仅用于引擎侧把
    // word.text 长度、word.difficulty 映射为特征（len_z/ext_difficulty），不参与 deltas 运算。
    /// 词长参考点（字符数）：len_z = clamp((len − ref)/scale, −1, 1)。
    #[serde(default = "default_cold_start_len_ref")]
    pub cold_start_len_ref: f64,
    /// 词长缩放（>0）：见上。
    #[serde(default = "default_cold_start_len_scale")]
    pub cold_start_len_scale: f64,
    /// 外部难度参考点（[1,10] 标度）：ext_difficulty==ref 时该项位移为 0。
    #[serde(default = "default_cold_start_extd_ref")]
    pub cold_start_extd_ref: f64,
    /// D₀ 偏移·词长权重（难词 D 升）。0.0=关闭。
    #[serde(default = "default_cold_start_weight_zero")]
    pub cold_start_d_len_weight: f64,
    /// D₀ 偏移·词素透明度权重（可分解→D 降，词根促进）。0.0=关闭。
    #[serde(default = "default_cold_start_weight_zero")]
    pub cold_start_d_morph_weight: f64,
    /// D₀ 偏移·外部难度权重。0.0=关闭。
    #[serde(default = "default_cold_start_weight_zero")]
    pub cold_start_d_extd_weight: f64,
    /// S₀ 偏移（对数域，乘性）·词长权重（难词 S 降）。0.0=关闭。
    #[serde(default = "default_cold_start_weight_zero")]
    pub cold_start_s_len_weight: f64,
    /// S₀ 偏移·词素透明度权重（可分解→S 升）。0.0=关闭。
    #[serde(default = "default_cold_start_weight_zero")]
    pub cold_start_s_morph_weight: f64,
    /// S₀ 偏移·外部难度权重。0.0=关闭。
    #[serde(default = "default_cold_start_weight_zero")]
    pub cold_start_s_extd_weight: f64,
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
pub(crate) fn default_mastery_window_size() -> u32 {
    20
}
pub(crate) fn default_streak_min_gap_ms() -> i64 {
    1_800_000
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
pub(crate) fn default_alpha_lapse_ramp_tau() -> f64 {
    // 默认关闭：同 alpha_ramp_tau，未声明即冻结语义
    0.0
}
pub(crate) fn default_forgetting_threshold() -> f64 {
    0.2
}
// === GSP 调度策略头默认值（全部 = 关闭，bit-exact legacy）===
pub(crate) fn default_gsp_success_grade() -> u32 {
    // 默认 3=Good：与 benchmark_adapter SUCCESS_QUALITY=0.7 落 Good 带逐位等价
    3
}
pub(crate) fn default_gsp_interval_cap_days() -> f64 {
    // 0=关闭：未声明该旋钮的配置（含 DB 历史快照）反序列化为精确旧语义
    0.0
}
pub(crate) fn default_gsp_graduation_streak() -> u32 {
    0
}
pub(crate) fn default_gsp_graduation_floor_days() -> f64 {
    30.0
}
pub(crate) fn default_gsp_young_retention() -> f64 {
    0.0
}
pub(crate) fn default_gsp_mature_retention() -> f64 {
    0.0
}
pub(crate) fn default_gsp_maturity_band_days() -> f64 {
    0.0
}
pub(crate) fn default_gsp_interval_fuzz() -> f64 {
    0.0
}
pub(crate) fn default_gsp_retire_after_reviews() -> u32 {
    0
}
pub(crate) fn default_gsp_retire_interval_days() -> f64 {
    365.0
}
pub(crate) fn default_gsp_retire_min_stability() -> f64 {
    0.0
}
pub(crate) fn default_gsp_retire_min_streak() -> u32 {
    0
}
pub(crate) fn default_difficulty_logit_weight() -> f64 {
    0.0
}
pub(crate) fn default_difficulty_logit_ref() -> f64 {
    5.0
}
pub(crate) fn default_pred_logit_base_scale() -> f64 {
    1.0
}
pub(crate) fn default_pred_logit_intercept() -> f64 {
    0.0
}
pub(crate) fn default_pred_logit_review_count_weight() -> f64 {
    0.0
}
pub(crate) fn default_cold_start_len_ref() -> f64 {
    7.0
}
pub(crate) fn default_cold_start_len_scale() -> f64 {
    4.0
}
pub(crate) fn default_cold_start_extd_ref() -> f64 {
    5.0
}
pub(crate) fn default_cold_start_weight_zero() -> f64 {
    // 所有冷启动权重默认 0.0=关闭（bit-exact legacy）
    0.0
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
            mastery_composite_threshold: 0.30,
            mastery_accuracy_threshold: 0.65,
            mastery_streak_threshold: 1,
            reviewing_threshold: 0.4,
            half_life_base_epsilon: 0.3,
            recall_risk_bonus: 0.2,
            recall_risk_threshold: 0.55,
            // FSRS 官方推荐 desired_retention，与 R(S,S)=0.9 语义自洽
            base_desired_retention: 0.9,
            mastery_window_size: 20,
            streak_min_gap_ms: 1_800_000,
            // FSRS-6 标准曲线无渐近线；保留为可调项
            forgetting_curve_floor: 0.0,
            w: default_w(),
            alpha_scale: 0.3,
            alpha_min: 0.1,
            alpha_max: 0.5,
            alpha_ramp_tau: 0.0,
            alpha_lapse_ramp_tau: 0.0,
            forgetting_threshold: 0.2,
            // GSP 调度策略头默认全关（bit-exact legacy）
            gsp_success_grade: 3,
            gsp_interval_cap_days: 0.0,
            gsp_graduation_streak: 0,
            gsp_graduation_floor_days: 30.0,
            gsp_young_retention: 0.0,
            gsp_mature_retention: 0.0,
            gsp_maturity_band_days: 0.0,
            gsp_interval_fuzz: 0.0,
            gsp_retire_after_reviews: 0,
            gsp_retire_interval_days: 365.0,
            gsp_retire_min_stability: 0.0,
            gsp_retire_min_streak: 0,
            difficulty_logit_weight: 0.0,
            difficulty_logit_ref: 5.0,
            pred_logit_base_scale: 1.0,
            pred_logit_intercept: 0.0,
            pred_logit_review_count_weight: 0.0,
            // 冷启动难度先验：ref/scale 中性值，6 个权重默认 0.0=关闭（bit-exact legacy）
            cold_start_len_ref: 7.0,
            cold_start_len_scale: 4.0,
            cold_start_extd_ref: 5.0,
            cold_start_d_len_weight: 0.0,
            cold_start_d_morph_weight: 0.0,
            cold_start_d_extd_weight: 0.0,
            cold_start_s_len_weight: 0.0,
            cold_start_s_morph_weight: 0.0,
            cold_start_s_extd_weight: 0.0,
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

