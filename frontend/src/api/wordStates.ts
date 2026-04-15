import { api } from './client';
import type { WordLearningState, WordStateOverview, BatchUpdateRequest } from '@/types/wordState';
import { WORD_STATES_DUE_DEFAULT_LIMIT } from '@/lib/constants';

export const wordStatesApi = {
  get(wordId: string) {
    return api.get<WordLearningState>(`/api/word-states/${wordId}`);
  },

  batchGet(wordIds: string[]) {
    return api.post<WordLearningState[]>('/api/word-states/batch', { wordIds });
  },

  getDueList(limit = WORD_STATES_DUE_DEFAULT_LIMIT) {
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
