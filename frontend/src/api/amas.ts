import { api } from './client';
import type { AmasUserState, AmasStrategy, AmasIntervention, LearningCurvePoint } from '@/types/amas';

export type ColdStartPhase = 'Classify' | 'Explore' | 'Exploit';

export interface MasteryEvaluation {
  wordId: string;
  state: string;
  masteryLevel: number;
  correctStreak: number;
  totalAttempts: number;
  nextReviewDate: string;
}

export interface AmasConfig {
  learningRate?: number;
  batchSize?: number;
  difficultyRange?: [number, number];
  reviewInterval?: number;
  [key: string]: unknown;
}

export interface AmasMetrics {
  totalUsers?: number;
  activeSessions?: number;
  avgAccuracy?: number;
  avgResponseTime?: number;
  [key: string]: unknown;
}

export interface MonitoringEvent {
  timestamp: string;
  eventType: string;
  data: Record<string, unknown>;
}

export const amasApi = {
  getState() {
    return api.get<AmasUserState>('/api/amas/state');
  },

  getStrategy() {
    return api.get<AmasStrategy>('/api/amas/strategy');
  },

  getPhase() {
    return api.get<{ phase: ColdStartPhase }>('/api/amas/phase');
  },

  getLearningCurve() {
    return api.get<{ curve: LearningCurvePoint[] }>('/api/amas/learning-curve');
  },

  getIntervention() {
    return api.get<{ interventions: AmasIntervention[] }>('/api/amas/intervention');
  },

  reset() {
    return api.post<{ reset: boolean }>('/api/amas/reset');
  },

  evaluateMastery(wordId: string) {
    return api.get<MasteryEvaluation>('/api/amas/mastery/evaluate', { wordId });
  },

  getConfig() {
    return api.get<AmasConfig>('/api/amas/config', undefined, { useAdminToken: true });
  },

  updateConfig(config: AmasConfig) {
    return api.put<{ updated: boolean }>('/api/amas/config', config, { useAdminToken: true });
  },

  getMetrics() {
    return api.get<AmasMetrics>('/api/amas/metrics', undefined, { useAdminToken: true });
  },

  getMonitoring(limit = 50) {
    return api.get<MonitoringEvent[]>('/api/amas/monitoring', { limit }, { useAdminToken: true });
  },
};
