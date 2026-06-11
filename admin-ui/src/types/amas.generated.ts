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
  iad?: IadConfig;
  ige?: IgeConfig;
  intervention?: InterventionConfig;
  learningStrategy?: LearningStrategyConfig;
  memoryModel?: MemoryModelConfig;
  modeling: ModelingConfig;
  monitoring: MonitoringConfig;
  mtp?: MtpConfig;
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
  kFactor: number;
  maxElo?: number;
  minElo?: number;
  noviceGameThreshold: number;
  noviceKMultiplier: number;
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
  ensembleEnabled: boolean;
  heuristicEnabled: boolean;
  /**
   * B38: Interference Aware Decay - 混淆词对干扰衰减
   */
  iadEnabled?: boolean;
  igeEnabled: boolean;
  mdmEnabled: boolean;
  /**
   * B37: Morpheme Transfer Prediction - 词素迁移预测
   */
  mtpEnabled?: boolean;
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
export interface IadConfig {
  confusionDecayRate?: number;
  confusionUpdateIncrement: number;
  interferencePenaltyCap: number;
  interferencePenaltyFactor: number;
  intervalShorteningFactor: number;
  maxConfusionPairs: number;
  newConfusionInitialScore: number;
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
  alphaMax?: number;
  alphaMin?: number;
  /**
   * 证据递减阻尼时间常数：成功复习时 alpha_eff(n)=1-(1-alpha)·e^{-(n-1)/tau}； 0.0=关闭（冻结语义），DB 旧快照/未声明配置反序列化即得旧行为。
   */
  alphaRampTau?: number;
  alphaScale?: number;
  baseDesiredRetention?: number;
  compositeWeightLong: number;
  compositeWeightMedium: number;
  compositeWeightShort: number;
  consolidationBonus: number;
  consolidationRateScale: number;
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
export interface MtpConfig {
  knownMorphemeDecay: number;
  maxKnownMorphemes: number;
  morphemeBonusCap: number;
  morphemeTransferCoeff: number;
  newMorphemeInitialCoeff: number;
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
