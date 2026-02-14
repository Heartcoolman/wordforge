import { api } from './client';
import type { Word, CreateWordRequest, BatchCreateResponse, ImportUrlResponse } from '@/types/word';
import type { PaginatedResponse } from '@/types/api';

export const wordsApi = {
  list: (params?: { page?: number; perPage?: number; search?: string }) =>
    api.get<PaginatedResponse<Word>>('/api/words', params),
  get: (id: string) => api.get<Word>(`/api/words/${id}`),
  create: (data: CreateWordRequest) => api.post<Word>('/api/words', data),
  update: (id: string, data: CreateWordRequest) => api.put<Word>(`/api/words/${id}`, data),
  delete: (id: string) => api.delete<{ deleted: boolean; id: string }>(`/api/words/${id}`),
  batchCreate: (words: CreateWordRequest[]) => api.post<BatchCreateResponse>('/api/words/batch', { words }),
  batchGet: (ids: string[]) => api.post<Word[]>('/api/words/batch-get', { ids }),
  count: () => api.get<{ total: number }>('/api/words/count'),
  importUrl: (url: string) => api.post<ImportUrlResponse>('/api/words/import-url', { url }),
};
