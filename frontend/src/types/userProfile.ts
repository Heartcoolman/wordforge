// 后端 RewardPreference 结构体: { rewardType: string }
export type RewardType = 'standard' | 'explorer' | 'achiever' | 'social';

export interface RewardPreference {
  rewardType: RewardType;
}

// 后端 CognitiveProfile (来自 AMAS): { memoryCapacity, processingSpeed, stability }
export interface CognitiveProfile {
  memoryCapacity: number;
  processingSpeed: number;
  stability: number;
}

// 后端 learning-style 返回: { style, scores: { visual, auditory, reading, kinesthetic } }
export interface LearningStyleScores {
  visual: number;
  auditory: number;
  reading: number;
  kinesthetic: number;
}

export type LearningStyleType = 'visual' | 'auditory' | 'reading' | 'kinesthetic';

export interface LearningStyle {
  style: LearningStyleType;
  scores: LearningStyleScores;
}

// 后端 chronotype 返回: { chronotype, preferredHours }
export interface Chronotype {
  chronotype: 'morning' | 'evening' | 'neutral';
  preferredHours: number[];
}

// 后端 HabitProfile: { preferredHours, medianSessionLengthMins, sessionsPerDay }
export interface HabitProfile {
  preferredHours: number[];
  medianSessionLengthMins: number;
  sessionsPerDay: number;
}

export interface HabitProfileRequest {
  preferredHours?: number[];
  medianSessionLengthMins?: number;
  sessionsPerDay?: number;
}
