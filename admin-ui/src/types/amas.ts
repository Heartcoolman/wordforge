// AMAS 类型入口
//
// 配置类（AmasConfig 及其所有子结构）由 schemars + json-schema-to-typescript 自动生成，
// 见 `amas.generated.ts`。任何后端 struct 增删字段都会反映到 generated 文件，
// 无需手工同步。运行时返回类型（ProcessResult、AmasStrategy 等）继续手写。

export type {
  AMASConfig,
  ClassifierConfig,
  ColdStartConfig,
  ConstraintConfig,
  EloConfig,
  EnsembleConfig,
  EvmConfig,
  FatigueDecayConfig,
  FeatureConfig,
  FeatureFlags,
  HeuristicConfig,
  IadConfig,
  IgeConfig,
  InterventionConfig,
  LearningStrategyConfig,
  MemoryModelConfig,
  ModelingConfig,
  MonitoringConfig,
  MtpConfig,
  ObjectiveWeights,
  RewardConfig,
  SspConfig,
  SspCostParams,
  SwdConfig,
  WordSelectorConfig,
} from './amas.generated';

// 兼容旧命名：现有代码以 `AmasConfig` / `AmasFeatureFlags` 等 PascalCase+Amas 前缀引用
import type * as Gen from './amas.generated';

/** @deprecated 使用 `AMASConfig`（从 amas.generated 导出） */
export type AmasConfig = Gen.AMASConfig;
/** @deprecated 使用 `FeatureFlags` */
export type AmasFeatureFlags = Gen.FeatureFlags;
/** @deprecated 使用 `EnsembleConfig` */
export type AmasEnsembleConfig = Gen.EnsembleConfig;
/** @deprecated 使用 `ModelingConfig` */
export type AmasModelingConfig = Gen.ModelingConfig;
/** @deprecated 使用 `ConstraintConfig` */
export type AmasConstraintConfig = Gen.ConstraintConfig;
/** @deprecated 使用 `MonitoringConfig` */
export type AmasMonitoringConfig = Gen.MonitoringConfig;
/** @deprecated 使用 `ColdStartConfig` */
export type AmasColdStartConfig = Gen.ColdStartConfig;
/** @deprecated 使用 `ObjectiveWeights` */
export type AmasObjectiveWeights = Gen.ObjectiveWeights;

// === 运行时返回类型（非配置）：保持手写 ===

export interface ProcessResult {
  sessionId: string;
  strategy: AmasStrategy;
  explanation: AmasExplanation;
  state: AmasUserState;
  wordMastery?: WordMastery;
  reward: AmasReward;
  coldStartPhase?: ColdStartPhase;
}

export interface ProcessEventRequest {
  wordId: string;
  isCorrect: boolean;
  responseTime: number;
  sessionId?: string;
  isQuit?: boolean;
  dwellTime?: number;
  pauseCount?: number;
  switchCount?: number;
  retryCount?: number;
  focusLossDuration?: number;
  interactionDensity?: number;
  pausedTimeMs?: number;
  hintUsed?: boolean;
  /** 易混淆词对（对齐后端 RawEvent.confused_with） */
  confusedWith?: string;
}

export interface BatchProcessResult {
  count: number;
  items: ProcessResult[];
}

export type ColdStartPhase = 'Classify' | 'Explore' | 'Exploit';

export interface AmasStrategy {
  difficulty: number;
  batchSize: number;
  newRatio: number;
  intervalScale: number;
  reviewMode: boolean;
}

export interface AmasExplanation {
  primaryReason: string;
  factors: AmasFactor[];
}

export interface AmasFactor {
  name: string;
  value: number;
  impact: string;
}

// 认知档案（对应后端 CognitiveProfile）
export interface AmasCognitiveProfile {
  memoryCapacity: number;
  processingSpeed: number;
  stability: number;
}

// 趋势状态（对应后端 TrendState）
export interface AmasTrendState {
  accuracyTrend: number;
  speedTrend: number;
  engagementTrend: number;
}

// 时段表现（对应后端 TemporalPerformance.hourly_stats[24]）
export interface AmasHourlyStat {
  accuracy: number;
  sessionCount: number;
  avgEngagementScore: number;
}

// 时段表现汇总（对应后端 TemporalPerformance）
export interface AmasTemporalPerformance {
  hourlyStats: AmasHourlyStat[];
  totalSessions: number;
}

// 习惯档案（对应后端 HabitProfile）
export interface AmasHabitProfile {
  preferredHours: number[];
  medianSessionLengthMins: number;
  sessionsPerDay: number;
  /** 24 小时分桶表现 */
  temporalPerformance?: AmasTemporalPerformance;
}

export interface AmasUserState {
  attention: number;
  fatigue: number;
  motivation: number;
  confidence: number;
  lastActiveAt?: string;
  sessionEventCount: number;
  totalEventCount: number;
  createdAt: string;
  cognitiveProfile?: AmasCognitiveProfile;
  trendState?: AmasTrendState;
  habitProfile?: AmasHabitProfile;
  /** 最近会话 ID（对齐后端 UserState.last_session_id） */
  lastSessionId?: string;
}

export interface WordMastery {
  wordId: string;
  memoryStrength: number;
  recallProbability: number;
  nextReviewIntervalSecs: number;
  masteryLevel: 'NEW' | 'LEARNING' | 'REVIEWING' | 'MASTERED' | 'FORGOTTEN';
}

export interface AmasReward {
  value: number;
  components: {
    accuracyReward: number;
    speedReward: number;
    fatiguePenalty: number;
    frustrationPenalty: number;
    /** 预期遗忘成本（对齐后端 RewardComponents.expected_forget_cost） */
    expectedForgetCost?: number;
  };
}

export interface AmasIntervention {
  type: 'rest' | 'encouragement' | 'focus' | 'continue';
  message: string;
  severity: 'warning' | 'info' | 'success';
}

export interface LearningCurvePoint {
  date: string;
  total: number;
  correct: number;
  accuracy: number;
}

export type MasteryState = 'NEW' | 'LEARNING' | 'REVIEWING' | 'MASTERED' | 'FORGOTTEN';

export interface MasteryEvaluation {
  wordId: string;
  state: MasteryState;
  masteryLevel: number;
  correctStreak: number;
  totalAttempts: number;
  nextReviewDate: string;
}

export interface AmasMetricsSnapshot {
  callCount: number;
  totalLatencyUs: number;
  errorCount: number;
}

export type AmasMetrics = Record<string, AmasMetricsSnapshot>;

export interface MonitoringEvent {
  timestamp: string;
  eventType: string;
  data: Record<string, unknown>;
}

export interface AmasStateStreamEvent {
  attention: number;
  fatigue: number;
  motivation: number;
  confidence: number;
  sessionEventCount: number;
  totalEventCount: number;
}
