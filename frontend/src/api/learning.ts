import { api } from './client';
import type {
  CreateSessionRequest,
  SessionResponse,
  StudyWordsResponse,
  NextWordsRequest,
  NextWordsResponse,
  AdjustWordsRequest,
  SyncProgressRequest,
  LearningSession,
  CompleteSessionRequest,
} from '@/types/learning';
import type { AmasStrategy } from '@/types/amas';

export const learningApi = {
  createSession(data?: CreateSessionRequest) {
    return api.post<SessionResponse>('/api/learning/session', data ?? {});
  },

  getStudyWords() {
    return api.get<StudyWordsResponse>('/api/learning/study-words');
  },

  getNextWords(data: NextWordsRequest) {
    return api.post<NextWordsResponse>('/api/learning/next-words', data);
  },

  completeSession(data: CompleteSessionRequest) {
    return api.post<LearningSession>('/api/learning/complete-session', data);
  },

  adjustWords(data?: AdjustWordsRequest) {
    return api.post<{ adjustedStrategy: AmasStrategy }>('/api/learning/adjust-words', data ?? {});
  },

  syncProgress(data: SyncProgressRequest) {
    const sanitized = { ...data };
    if (sanitized.totalQuestions != null) sanitized.totalQuestions = Math.round(sanitized.totalQuestions);
    if (sanitized.contextShifts != null) sanitized.contextShifts = Math.round(sanitized.contextShifts);
    return api.post<LearningSession>('/api/learning/sync-progress', sanitized);
  },
};
