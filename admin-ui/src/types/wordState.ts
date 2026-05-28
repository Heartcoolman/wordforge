// P3#7：后端 wire 序列化改为 lowercase（`#[serde(rename_all = "lowercase")]`）
export type WordStateType = 'new' | 'learning' | 'reviewing' | 'mastered' | 'forgotten';

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
