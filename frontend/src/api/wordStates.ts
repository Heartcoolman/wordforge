import { api } from './client';
import type { WordLearningState, WordStateOverview, BatchUpdateRequest } from '@/types/wordState';

export const wordStatesApi = {
  get(wordId: string) {
    return api.get<WordLearningState>(`/api/word-states/${wordId}`);
  },

  batchGet(wordIds: string[]) {
    return api.post<WordLearningState[]>('/api/word-states/batch', { wordIds });
  },

  getDueList(limit = 50) {
    return api.get<WordLearningState[]>('/api/word-states/due/list', { limit });
  },

  getOverview() {
    return api.get<WordStateOverview>('/api/word-states/stats/overview');
  },

  batchUpdate(data: BatchUpdateRequest) {
    return api.post<{ updated: number }>('/api/word-states/batch-update', data);
  },

  markMastered(wordId: string) {
    return api.post<WordLearningState>(`/api/word-states/${wordId}/mark-mastered`);
  },

  reset(wordId: string) {
    return api.post<WordLearningState>(`/api/word-states/${wordId}/reset`);
  },
};
