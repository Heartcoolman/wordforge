export interface ProcessResult {
  sessionId: string;
  strategy: AmasStrategy;
  explanation: AmasExplanation;
  state: AmasUserState;
  wordMastery?: WordMastery;
  reward: AmasReward;
  coldStartPhase?: 'Classify' | 'Explore' | 'Exploit';
}

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

export interface AmasUserState {
  attention: number;
  fatigue: number;
  motivation: number;
  confidence: number;
  lastActiveAt?: string;
  sessionEventCount: number;
  totalEventCount: number;
  createdAt: string;
}

export interface WordMastery {
  wordId: string;
  memoryStrength: number;
  recallProbability: number;
  nextReviewIntervalSecs: number;
  masteryLevel: 'New' | 'Learning' | 'Reviewing' | 'Mastered';
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
  type: 'break' | 'encouragement' | 'review' | 'difficulty_adjustment';
  message: string;
  severity: 'low' | 'medium' | 'high';
}

export interface LearningCurvePoint {
  date: string;
  total: number;
  correct: number;
  accuracy: number;
}
