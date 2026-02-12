export type WordStateType = 'NEW' | 'LEARNING' | 'REVIEWING' | 'MASTERED' | 'FORGOTTEN';

export interface WordLearningState {
  userId: string;
  wordId: string;
  state: WordStateType;
  masteryLevel: number;
  nextReviewDate?: string;
  halfLife: number;
  correctStreak: number;
  totalAttempts: number;
  updatedAt: string;
}

export interface WordStateOverview {
  newCount: number;
  learning: number;
  reviewing: number;
  mastered: number;
  forgotten: number;
}

export interface BatchUpdateRequest {
  updates: { wordId: string; state?: WordStateType; masteryLevel?: number }[];
}
