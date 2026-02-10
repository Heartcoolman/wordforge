import { api } from './client';
import type { SessionResponse, StudyWordsResponse, NextWordsRequest, NextWordsResponse, SyncProgressRequest, LearningSession } from '@/types/learning';
import type { AmasStrategy } from '@/types/amas';

export const learningApi = {
  createSession() {
    return api.post<SessionResponse>('/api/learning/session');
  },

  getStudyWords() {
    return api.post<StudyWordsResponse>('/api/learning/study-words');
  },

  getNextWords(data: NextWordsRequest) {
    return api.post<NextWordsResponse>('/api/learning/next-words', data);
  },

  adjustWords(data: { userState?: string; recentPerformance?: number }) {
    return api.post<{ adjustedStrategy: AmasStrategy }>('/api/learning/adjust-words', data);
  },

  syncProgress(data: SyncProgressRequest) {
    return api.post<LearningSession>('/api/learning/sync-progress', data);
  },
};
