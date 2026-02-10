export type WordStateType = 'New' | 'Learning' | 'Reviewing' | 'Mastered';

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
  new: number;
  learning: number;
  reviewing: number;
  mastered: number;
}

export interface BatchUpdateRequest {
  updates: { wordId: string; state?: WordStateType; masteryLevel?: number }[];
}
