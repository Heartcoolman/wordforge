// AUTO-GENERATED — do not edit. Run: npm run gen:amas-types
// Source: admin-ui/src/types/amas.schema.json (由后端 schemars 导出)
// 后端事实源：src/amas/config.rs (AMASConfig) + 全部子结构体

export interface AMASConfig {
  classifier?: ClassifierConfig;
  coldStart: ColdStartConfig;
  constraints: ConstraintConfig;
  elo?: EloConfig;
  ensemble: EnsembleConfig;
  evm?: EvmConfig;
  fatigueDecay?: FatigueDecayConfig;
  feature?: FeatureConfig;
  featureFlags: FeatureFlags;
  heuristic?: HeuristicConfig;
  ige?: IgeConfig;
  intervention?: InterventionConfig;
  learningStrategy?: LearningStrategyConfig;
  memoryModel?: MemoryModelConfig;
  modeling: ModelingConfig;
  monitoring: MonitoringConfig;
  objectiveWeights: ObjectiveWeights;
  reward?: RewardConfig;
  ssp?: SspConfig;
  swd?: SwdConfig;
  wordSelector?: WordSelectorConfig;
}
export interface ClassifierConfig {
  fastLearnerThreshold: number;
  memoryCapacityWeight: number;
  processingSpeedWeight: number;
  stabilityWeight: number;
  stableLearnerThreshold: number;
}
export interface ColdStartConfig {
  classifyToExploreConfidence: number;
  classifyToExploreEvents: number;
  exploreToExploitEvents: number;
}
export interface ConstraintConfig {
  highFatigueThreshold: number;
  lowAttentionThreshold: number;
  lowMotivationDifficultyDrop?: number;
  lowMotivationRatioDrop?: number;
  lowMotivationThreshold: number;
  maxBatchSizeWhenFatigued: number;
  maxDifficultyWhenFatigued: number;
  maxNewRatioWhenFatigued: number;
  minDifficulty?: number;
}
export interface EloConfig {
  defaultElo: number;
  /**
   * 开关：true 时按近期残差趋势动态调 K。
   */
  kDynamicEnabled?: boolean;
  kFactor: number;
  /**
   * K 乘子上界（防放大噪声）。
   */
  kMaxFactor?: number;
  /**
   * K 乘子下界（防过度降 K 致停滞）。
   */
  kMinFactor?: number;
  /**
   * 稳态阻尼：残差震荡(|趋势|→0)时把 K 乘子下压的量（降噪）。
   */
  kTrendDamp?: number;
  /**
   * 趋势增益：|趋势| 每单位放大 K 的系数（连续同向误差 → 增 K 追漂移）。
   */
  kTrendGain?: number;
  /**
   * 残差趋势 EWMA 权重 ∈ (0,1]：越大越看重最近一次残差。
   */
  kTrendWeight?: number;
  maxElo?: number;
  minElo?: number;
  noviceGameThreshold: number;
  noviceKMultiplier: number;
  /**
   * 开关：true 时 ZPD 选词读「选词链」(rating_select，延迟快照)，更新写「估计链」(rating)， 消除「选择依赖被估计量」的耦合偏差。难度消费者(difflogit/analytics)恒读估计链。
   */
  parallelEloEnabled?: boolean;
  /**
   * 选词链延迟刷新间隔（按该词全局对局数）：每 N 局把选词链快照到估计链当前值。
   */
  parallelEloRefreshGames?: number;
  wordKFactorRatio?: number;
  zpdGaussianSigma: number;
  zpdOptimalOffset: number;
}
export interface EnsembleConfig {
  baseWeightHeuristic: number;
  baseWeightIge: number;
  baseWeightSwd: number;
  blendMax: number;
  blendScale: number;
  minWeight: number;
  warmupHeuristicBoost?: number;
  warmupSamples: number;
}
export interface EvmConfig {
  diversityBonusCap?: number;
  diversityGrowthRate?: number;
  diversityLogDivisor?: number;
}
export interface FatigueDecayConfig {
  decayStartThresholdSecs: number;
  decayTimeConstantSecs: number;
  fullResetThresholdSecs: number;
}
export interface FeatureConfig {
  confidenceNegativeSignal: number;
  confidencePositiveSignal: number;
  hintPenalty: number;
  incorrectQualityScale?: number;
  motivationNegativeSignal: number;
  motivationPositiveSignal: number;
  qualityAccuracyWeight: number;
  qualitySpeedWeight: number;
  temporalBoostBase: number;
  temporalBoostMax: number;
  temporalBoostMin: number;
  temporalBoostScale: number;
  temporalProfileAlpha: number;
  trustBaseLearningRate: number;
  trustWeightBlend: number;
}
export interface FeatureFlags {
  /**
   * ② 混淆隔离排程（Phase 1b）：开启后选词调用方将已出现词的高分混淆对端注入 SessionSelectionContext.confusion_exclude_word_ids（评分乘 confusion_isolation_dampen）。 默认 false → 调用方不填充 → 选词 bit-exact legacy。
   */
  confusionIsolationEnabled?: boolean;
  ensembleEnabled: boolean;
  heuristicEnabled: boolean;
  igeEnabled: boolean;
  mdmEnabled: boolean;
  /**
   * ③ 跨题型多痕迹（Phase 2）：开启后 mastery 状态按 question_mode 分痕迹 （`mastery:{word}:{mode}`），选词按 min-recall 跨痕迹聚合。默认 false → 单一 `mastery:{word}` 键、bit-exact legacy。
   */
  multiTraceEnabled?: boolean;
  /**
   * SSP-MMC: 最优间隔调度（离线 DP 预计算策略表）
   */
  sspEnabled?: boolean;
  swdEnabled: boolean;
}
export interface HeuristicConfig {
  accuracySpeedDifficultyBoost: number;
  coldStartBatchSize: number;
  coldStartDifficulty: number;
  coldStartEventThreshold: number;
  coldStartNewRatio: number;
  confidenceBase: number;
  confidenceDecayCap: number;
  confidenceDecayScale: number;
  confidenceMin: number;
  lowAccuracyDifficultyDrop: number;
  lowAccuracyRatioDrop: number;
  lowMotivationDifficultyDrop: number;
  lowMotivationMaxBatch: number;
}
export interface IgeConfig {
  batchSize: number;
  defaultConfidence: number;
  difficultyBinCount?: number;
  intervalScale: number;
  pretrainedDifficultyRewards?: number[] | null;
  pretrainedRatioRewards?: number[] | null;
  ratioBinCount?: number;
  ucbConfidenceCoeff: number;
}
export interface InterventionConfig {
  attentionAlertThreshold: number;
  fatigueAlertThreshold: number;
  motivationAlertThreshold: number;
}
export interface LearningStrategyConfig {
  confidenceBoostThreshold: number;
  confidenceDifficultyBoost: number;
  crossSessionHighAccuracy: number;
  crossSessionHighDifficulty: number;
  crossSessionLowDifficulty: number;
  crossSessionMediumAccuracy: number;
  crossSessionMediumDifficulty: number;
  difficultyBoostStep: number;
  difficultyDropStep: number;
  fatigueBatchScale: number;
  fatigueDifficultyDrop: number;
  fatigueReductionThreshold: number;
  motivationRatioBoost: number;
  motivationRatioThreshold: number;
  ratioBoostStep: number;
  ratioDropStep: number;
  sessionBoostAccuracy: number;
  sessionDropAccuracy: number;
  sprintMasteryRatio: number;
  sprintNewRatio: number;
}
export interface MemoryModelConfig {
  /**
   * 双腿信任调度·失败腿时间常数 τ_f：失败（Again）时 alpha_eff(f)=1-(1-alpha)·e^{-(f-1)/tau}，f=含本次的累计 lapse 数 （首错 f=1 即 no-op = 偶发失误保护）；0.0=关闭（冻结语义）。
   */
  alphaLapseRampTau?: number;
  alphaMax?: number;
  alphaMin?: number;
  /**
   * 双腿信任调度·成功腿时间常数 τ_s：成功复习（grade≥2）时 alpha_eff(k)=1-(1-alpha)·e^{-(k-1)/tau}，k=本次记账后的 correct_streak （失败清零→阻尼重启）；0.0=关闭（冻结语义），DB 旧快照/未声明配置反序列化即得旧行为。
   */
  alphaRampTau?: number;
  alphaScale?: number;
  baseDesiredRetention?: number;
  /**
   * D₀ 偏移·外部难度权重。0.0=关闭。
   */
  coldStartDExtdWeight?: number;
  /**
   * D₀ 偏移·词长权重（难词 D 升）。0.0=关闭。
   */
  coldStartDLenWeight?: number;
  /**
   * D₀ 偏移·词素透明度权重（可分解→D 降，词根促进）。0.0=关闭。
   */
  coldStartDMorphWeight?: number;
  /**
   * 外部难度参考点（[1,10] 标度）：ext_difficulty==ref 时该项位移为 0。
   */
  coldStartExtdRef?: number;
  /**
   * 词长参考点（字符数）：len_z = clamp((len − ref)/scale, −1, 1)。
   */
  coldStartLenRef?: number;
  /**
   * 词长缩放（>0）：见上。
   */
  coldStartLenScale?: number;
  /**
   * S₀ 偏移·外部难度权重。0.0=关闭。
   */
  coldStartSExtdWeight?: number;
  /**
   * S₀ 偏移（对数域，乘性）·词长权重（难词 S 降）。0.0=关闭。
   */
  coldStartSLenWeight?: number;
  /**
   * S₀ 偏移·词素透明度权重（可分解→S 升）。0.0=关闭。
   */
  coldStartSMorphWeight?: number;
  compositeWeightLong: number;
  compositeWeightMedium: number;
  compositeWeightShort: number;
  consolidationBonus: number;
  consolidationRateScale: number;
  /**
   * difficulty logit 项参考点（[1,10] 标度）：word_difficulty==REF 时位移为 0。默认 5.0。
   */
  difficultyLogitRef?: number;
  /**
   * 预测/调度 recall **读出**在 logit 域加 `β·(REF − word_difficulty[1,10])`：难词降 p、易词升 p， 补 FSRS 二元映射下「预测随难度扁平」的区分度/校准残差（maimemo TEST: AUC+0.015 反超 dhp、 logLoss−0.005）。仅作用于 `recall_probability_predicted` 读出路径，**不入内部 S/D 更新** （update_strength 的 r 仍用纯 recall_probability）。0.0=关闭（bit-exact legacy）； 无每词难度的调用点传 None → 自然 no-op。
   */
  difficultyLogitWeight?: number;
  /**
   * DEPRECATED（FSRS-6 起）：遗忘曲线 decay 即 trainable 参数 `w[20]`（`curve_decay()`）， 本字段仅为旧配置/DB 快照反序列化兼容保留，运行时不再读取。
   */
  forgettingCurveDecay?: number;
  /**
   * DEPRECATED（FSRS-6 起）：遗忘曲线 factor 由 `w[20]` 派生（`curve_factor()`）， 本字段仅为旧配置/DB 快照反序列化兼容保留，运行时不再读取。
   */
  forgettingCurveFactor?: number;
  /**
   * 2021 MaiMemo study: forgetting curve has non-zero asymptote R→floor (not 0) FSRS-6 标准曲线无渐近线，默认 0.0；保留为可调项。
   */
  forgettingCurveFloor?: number;
  forgettingThreshold?: number;
  /**
   * 毕业下限（天）。在 scale 之后、cap 之前施加 max(interval, floor)（随 streak 关闭而失效）。
   */
  gspGraduationFloorDays?: number;
  /**
   * 毕业连击阈值 k。0=关闭。当词的 correct_streak >= k 时，调度区间获得 floor 下限。
   */
  gspGraduationStreak?: number;
  /**
   * 调度区间硬帽（天）。0=关闭。作用于 interval_scale 之后，与既有 90 天硬帽复合 min(90, cap)。
   */
  gspIntervalCapDays?: number;
  /**
   * 复习负载平滑：确定性区间抖动幅度。0=关闭。>0 时在 cap 之后施加 days·(1+fuzz·u)， u∈[-1,1) 由 stability/review_count 派生（见 GSP_SPEC §7），错峰削平同步复习波。
   */
  gspIntervalFuzz?: number;
  /**
   * 成熟卡（stability >= band）目标保持率（随 band 关闭而失效）。
   */
  gspMatureRetention?: number;
  /**
   * 成熟度分带阈值（天，按 stability）。0=关闭。>0 时 interval 求解的目标保持率由 young/mature 替换自适应 desired_retention（仅区间求解口径，预测/recall 路径不受影响）。
   */
  gspMaturityBandDays?: number;
  /**
   * 二元成功复习映射的 FSRS grade：3=Good（旧默认，首评 S0=w[2]，bit-exact legacy）； 4=Easy（首评 S0=w[3] + 成功复习带 w[16] easy_bonus + 进入 FSRS-6 faithful 状态路径， 见 mdm.rs §3.5）。FSRS-6 公版 21 维 w 与 grade=4 共拟合 → 候选用 4 恢复预测腿校准。 入口 clamp：非 {3,4} 一律夹回 3。失败恒映射 1=Again。
   */
  gspSuccessGrade?: number;
  /**
   * 年轻卡（stability < band）目标保持率（随 band 关闭而失效）。
   */
  gspYoungRetention?: number;
  halfLifeBaseEpsilon: number;
  halfLifePower?: number;
  halfLifeTimeUnitSecs: number;
  highAccuracyRetentionBoost?: number;
  highAccuracyThreshold?: number;
  highFatigueRetentionDrop?: number;
  highFatigueThreshold?: number;
  longTermLearningRate: number;
  lowMotivationRetentionDrop?: number;
  lowMotivationThreshold?: number;
  masteryAccuracyThreshold: number;
  masteryCompositeThreshold: number;
  masteryStreakThreshold: number;
  masteryWindowSize?: number;
  maxIntervalDays?: number;
  mediumTermLearningRate: number;
  minIntervalSecs?: number;
  passiveDecayHalfLifeDays?: number;
  passiveDecayPower?: number;
  /**
   * 读出 recall 在 logit 域施加：`logit(p) = base_scale·logit(p_fsrs) + intercept + β_diff·(REF−D) + β_nrev·ln(1+review_count)`。前两项是 Platt/温度重校准（FSRS-6 在真实 词汇数据上略过自信），β_nrev 补复习次数残差（stability 未完全吸收）。系数**按部署离线拟合** （benchmarks/maimemo/pred_calib.py，maimemo held-out: AUC+0.0116/logLoss−0.0084/ECE 0.022→0.016， 5/5 seed），**不跨域迁移**（synthetic 退化，须 prod_replay 复核）。仅作用 `recall_probability_predicted` 读出路径，不入 S/D 更新。默认 base_scale=1/intercept=0/β_nrev=0 → 退化为纯 difficulty_logit。
   */
  predLogitBaseScale?: number;
  predLogitIntercept?: number;
  predLogitReviewCountWeight?: number;
  recallRiskBonus: number;
  recallRiskThreshold: number;
  retentionMax?: number;
  retentionMin?: number;
  reviewingThreshold: number;
  shortTermLearningRate: number;
  stabilityBaseDays?: number;
  streakMinGapMs?: number;
  /**
   * 反序列化兼容 19 维 FSRS-5 旧配置：自动迁移（w19=0 维持旧同日公式，w20=0.5 维持旧 decay）。
   *
   * @minItems 21
   * @maxItems 21
   */
  w?: [
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    number
  ];
}
export interface ModelingConfig {
  attentionSmoothing: number;
  /**
   * 认知画像平滑系数（EMA alpha）
   */
  cognitiveProfileAlpha?: number;
  confidenceDecay: number;
  /**
   * engagement 中焦点丢失时长的归一化基准（毫秒）
   */
  engagementFocusLossBaseMs?: number;
  /**
   * engagement 中焦点丢失惩罚的上限
   */
  engagementFocusLossPenaltyMax?: number;
  /**
   * engagement 中暂停次数的惩罚系数（每次暂停扣分）
   */
  engagementPausePenalty?: number;
  /**
   * engagement 中暂停惩罚的上限
   */
  engagementPausePenaltyMax?: number;
  /**
   * engagement 中切换次数的惩罚系数（每次切换扣分）
   */
  engagementSwitchPenalty?: number;
  /**
   * engagement 中切换惩罚的上限
   */
  engagementSwitchPenaltyMax?: number;
  fatigueIncreaseRate: number;
  /**
   * 用户退出时的疲劳增加量
   */
  fatigueQuitIncrease?: number;
  fatigueRecoveryRate: number;
  minConfidence: number;
  motivationMomentum: number;
  /**
   * response_speed 归一化的最大响应时间（毫秒）
   */
  responseSpeedMaxMs?: number;
  /**
   * 趋势状态平滑系数（EMA alpha）
   */
  trendAlpha?: number;
  /**
   * 视觉疲劳信号在混合公式中的权重 (0.0-1.0) 行为信号权重 = 1.0 - visual_fatigue_weight
   */
  visualFatigueWeight: number;
}
export interface MonitoringConfig {
  metricsFlushIntervalSecs: number;
  sampleRate: number;
}
export interface ObjectiveWeights {
  accuracy: number;
  fatigue: number;
  frustration: number;
  retention: number;
  speed: number;
}
export interface RewardConfig {
  expectedForgetCostWeight?: number;
  fatiguePenaltyScale: number;
  fatiguePenaltyThreshold: number;
  frustrationPenaltyScale: number;
  frustrationPenaltyThreshold: number;
  speedRewardScale: number;
}
export interface SspConfig {
  /**
   * 离散化底数（与墨墨一致）
   */
  base: number;
  /**
   * DP 收敛阈值
   */
  convergenceThreshold: number;
  /**
   * 参数化代价函数：recall/forget cost 随 S/D 调制
   */
  costParams?: SspCostParams;
  /**
   * 遗忘后难度增量（映射到 D ∈ [1,10]）
   */
  difficultyOffsetOnLapse: number;
  /**
   * Bellman 折扣因子 γ ∈ [0,1]，防止无穷代价堆积
   */
  discountFactor?: number;
  /**
   * 双网格离散化：小 S 用 log 间距，大 S 用线性间距
   */
  dualGridEnabled?: boolean;
  /**
   * 双网格切换阈值（天）
   */
  dualGridThresholdDays?: number;
  forgetCost: number;
  /**
   * 线性间距步长（天）
   */
  linearStepDays?: number;
  /**
   * stability log 等比 bins 的最大指数
   */
  maxIndex: number;
  /**
   * DP 最大迭代次数
   */
  maxIterations: number;
  /**
   * stability log 等比 bins 的最小指数
   */
  minIndex: number;
  /**
   * 3D 求解器：目标保持率搜索上界
   */
  rMax?: number;
  /**
   * 3D 求解器：目标保持率搜索下界
   */
  rMin?: number;
  /**
   * 3D 求解器：保持率离散步长
   */
  rStep?: number;
  recallCost: number;
  /**
   * 评分概率分布 [Hard, Good, Easy]
   *
   * @minItems 3
   * @maxItems 3
   */
  reviewRatingProbs?: [number, number, number];
  /**
   * 目标 stability（天）：halflife 达到此值视为"长期记住"
   */
  targetStabilityDays: number;
}
export interface SspCostParams {
  /**
   * difficulty 对 forget cost 的线性调制系数
   */
  forgetDCoeff?: number;
  /**
   * stability 对 forget cost 的线性调制系数
   */
  forgetSCoeff?: number;
  /**
   * difficulty 对 recall cost 的线性调制系数
   */
  recallDCoeff?: number;
  /**
   * stability 对 recall cost 的线性调制系数
   */
  recallSCoeff?: number;
}
export interface SwdConfig {
  fallbackConfidence: number;
  historyFilterThreshold: number;
  maxHistorySize: number;
  similarityCacheTtlSecs?: number;
}
export interface WordSelectorConfig {
  /**
   * ② 混淆隔离（Phase 1b）：命中 SessionSelectionContext.confusion_exclude_word_ids 的词 评分乘此系数。1.0=no-op（默认，bit-exact legacy）；<1 降优先级、>=1 无意义（validate 限 [0,1]）。
   */
  confusionIsolationDampen?: number;
  /**
   * ② 混淆隔离：调用方筛选已出现词的混淆对端时的最小 score 阈值（仅调用侧用，select_words 不读）。
   */
  confusionMinScore?: number;
  errorProneBonus: number;
  newWordGaussianSigma: number;
  optimalRecallCenter?: number;
  optimalRecallSigma?: number;
  recallMasteredThreshold: number;
  recentlyMasteredBonus: number;
  reviewUcbMaxBonus: number;
  reviewUcbWeight: number;
  sigmoidSteepness?: number;
  spacingCooldownSecs?: number;
}
