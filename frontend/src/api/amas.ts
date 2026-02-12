import { api, connectAmasStateStream } from './client';
import type {
  AmasStateStreamEvent,
  AmasUserState,
  AmasStrategy,
  AmasIntervention,
  LearningCurvePoint,
  ColdStartPhase,
  MasteryEvaluation,
  AmasConfig,
  AmasMetrics,
  MonitoringEvent,
} from '@/types/amas';
import { AMAS_MONITORING_DEFAULT_LIMIT } from '@/lib/constants';

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

  reportVisualFatigue(score: number) {
    return api.post<AmasUserState>('/api/amas/visual-fatigue', { score });
  },

  getConfig() {
    return api.get<AmasConfig>('/api/admin/amas/config', undefined, { useAdminToken: true });
  },

  updateConfig(config: AmasConfig) {
    return api.put<{ updated: boolean }>('/api/admin/amas/config', config, { useAdminToken: true });
  },

  getMetrics() {
    return api.get<AmasMetrics>('/api/admin/amas/metrics', undefined, { useAdminToken: true });
  },

  getMonitoring(limit = AMAS_MONITORING_DEFAULT_LIMIT) {
    return api.get<MonitoringEvent[]>('/api/admin/amas/monitoring', { limit }, { useAdminToken: true });
  },

  subscribeStateEvents(onState: (event: AmasStateStreamEvent) => void) {
    return connectAmasStateStream(onState);
  },
};
