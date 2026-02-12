import type { Word } from './word';

export interface LearningSession {
  id: string;
  userId: string;
  status: 'active' | 'completed' | 'abandoned';
  targetMasteryCount: number;
  totalQuestions: number;
  actualMasteryCount: number;
  contextShifts: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateSessionRequest {
  targetMasteryCount?: number;
}

export interface CrossSessionHint {
  prevAccuracy: number;
  prevMasteredCount: number;
  gapMinutes: number;
  suggestedDifficulty: number;
  errorProneWordIds: string[];
  recentlyMasteredWordIds: string[];
}

export interface SessionResponse {
  sessionId: string;
  status: LearningSession['status'];
  resumed: boolean;
  targetMasteryCount: number;
  crossSessionHint?: CrossSessionHint;
}

export interface StudyWordsResponse {
  words: Word[];
  strategy: {
    difficultyRange: [number, number];
    newRatio: number;
    batchSize: number;
  };
}

export interface SessionPerformanceData {
  recentAccuracy: number;
  overallAccuracy: number;
  masteredCount: number;
  targetMasteryCount: number;
  errorProneWordIds: string[];
}

export interface NextWordsRequest {
  excludeWordIds: string[];
  masteredWordIds?: string[];
  sessionId?: string;
  sessionPerformance?: SessionPerformanceData;
}

export interface NextWordsResponse {
  words: Word[];
  batchSize: number;
}

export type AdjustUserState = 'fatigued' | 'frustrated' | 'distracted' | 'engaged' | 'confident' | 'focused';

export interface AdjustWordsRequest {
  recentPerformance?: number;
  userState?: AdjustUserState;
}

export interface SyncProgressRequest {
  sessionId: string;
  totalQuestions?: number;
  contextShifts?: number;
}

export interface CompleteSessionRequest {
  sessionId: string;
  masteredWordIds: string[];
  errorProneWordIds: string[];
  avgResponseTimeMs: number;
}

export interface CompleteSessionResponse {
  session: LearningSession;
}
