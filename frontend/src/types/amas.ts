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

// 习惯档案（对应后端 HabitProfile）
export interface AmasHabitProfile {
  preferredHours: number[];
  medianSessionLengthMins: number;
  sessionsPerDay: number;
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

export interface AmasFeatureFlags {
  ensembleEnabled: boolean;
  heuristicEnabled: boolean;
  igeEnabled: boolean;
  swdEnabled: boolean;
  mdmEnabled: boolean;
}

export interface AmasEnsembleConfig {
  baseWeightHeuristic: number;
  baseWeightIge: number;
  baseWeightSwd: number;
  warmupSamples: number;
  blendScale: number;
  blendMax: number;
  minWeight: number;
}

export interface AmasModelingConfig {
  attentionSmoothing: number;
  confidenceDecay: number;
  minConfidence: number;
  fatigueIncreaseRate: number;
  fatigueRecoveryRate: number;
  motivationMomentum: number;
  visualFatigueWeight: number;
}

export interface AmasConstraintConfig {
  highFatigueThreshold: number;
  lowAttentionThreshold: number;
  lowMotivationThreshold: number;
  maxBatchSizeWhenFatigued: number;
  maxNewRatioWhenFatigued: number;
  maxDifficultyWhenFatigued: number;
}

export interface AmasMonitoringConfig {
  sampleRate: number;
  metricsFlushIntervalSecs: number;
}

export interface AmasColdStartConfig {
  classifyToExploreEvents: number;
  classifyToExploreConfidence: number;
  exploreToExploitEvents: number;
}

export interface AmasObjectiveWeights {
  retention: number;
  accuracy: number;
  speed: number;
  fatigue: number;
  frustration: number;
}

export interface AmasConfig {
  featureFlags: AmasFeatureFlags;
  ensemble: AmasEnsembleConfig;
  modeling: AmasModelingConfig;
  constraints: AmasConstraintConfig;
  monitoring: AmasMonitoringConfig;
  coldStart: AmasColdStartConfig;
  objectiveWeights: AmasObjectiveWeights;
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
