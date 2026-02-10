import type { ProcessResult } from './amas';

export interface LearningRecord {
  id: string;
  userId: string;
  wordId: string;
  isCorrect: boolean;
  responseTimeMs: number;
  sessionId?: string;
  createdAt: string;
}

export interface CreateRecordRequest {
  wordId: string;
  isCorrect: boolean;
  responseTimeMs: number;
  sessionId?: string;
  isQuit?: boolean;
  dwellTimeMs?: number;
  pauseCount?: number;
  switchCount?: number;
  retryCount?: number;
  focusLossDurationMs?: number;
  interactionDensity?: number;
  pausedTimeMs?: number;
  hintUsed?: boolean;
}

export interface RecordResponse {
  record: LearningRecord;
  amasResult: ProcessResult;
}

export interface RecordStatistics {
  total: number;
  correct: number;
  accuracy: number;
}

export interface EnhancedStatistics {
  total: number;
  correct: number;
  accuracy: number;
  streak: number;
  daily: DailyStatistic[];
}

export interface DailyStatistic {
  date: string;
  total: number;
  correct: number;
  accuracy: number;
}
