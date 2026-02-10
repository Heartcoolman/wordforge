import type { Word } from './word';

export interface LearningSession {
  id: string;
  userId: string;
  status: 'Active' | 'Completed' | 'Abandoned';
  targetMasteryCount: number;
  totalQuestions: number;
  actualMasteryCount: number;
  contextShifts: number;
  createdAt: string;
  updatedAt: string;
}

export interface SessionResponse {
  sessionId: string;
  status: LearningSession['status'];
  resumed: boolean;
}

export interface StudyWordsResponse {
  words: Word[];
  strategy: {
    difficultyRange: [number, number];
    newRatio: number;
    batchSize: number;
  };
}

export interface NextWordsRequest {
  excludeWordIds: string[];
  masteredWordIds?: string[];
}

export interface NextWordsResponse {
  words: Word[];
  batchSize: number;
}

export interface SyncProgressRequest {
  sessionId: string;
  totalQuestions?: number;
  contextShifts?: number;
}
