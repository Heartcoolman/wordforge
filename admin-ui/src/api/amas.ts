import { api, connectAmasStateStream } from './client';
import type {
  AmasStateStreamEvent,
  AmasUserState,
  ProcessResult,
  ProcessEventRequest,
  BatchProcessResult,
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

function sanitizeProcessEventPayload(payload: ProcessEventRequest): ProcessEventRequest {
  return {
    ...payload,
    responseTime: Math.round(payload.responseTime),
    dwellTime: payload.dwellTime != null ? Math.round(payload.dwellTime) : undefined,
    pauseCount: payload.pauseCount != null ? Math.round(payload.pauseCount) : undefined,
    switchCount: payload.switchCount != null ? Math.round(payload.switchCount) : undefined,
    retryCount: payload.retryCount != null ? Math.round(payload.retryCount) : undefined,
    focusLossDuration: payload.focusLossDuration != null ? Math.round(payload.focusLossDuration) : undefined,
    pausedTimeMs: payload.pausedTimeMs != null ? Math.round(payload.pausedTimeMs) : undefined,
  };
}

export const amasApi = {
  processEvent(payload: ProcessEventRequest) {
    return api.post<ProcessResult>('/api/amas/process-event', sanitizeProcessEventPayload(payload));
  },

  batchProcess(events: ProcessEventRequest[]) {
    return api.post<BatchProcessResult>('/api/amas/batch-process', {
      events: events.map(sanitizeProcessEventPayload),
    });
  },

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
