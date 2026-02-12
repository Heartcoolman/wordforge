import { api } from './client';
import type { Wordbook, CreateWordbookRequest } from '@/types/wordbook';
import type { Word } from '@/types/word';
import type { PaginatedResponse } from '@/types/api';

export const wordbooksApi = {
  getSystem() {
    return api.get<Wordbook[]>('/api/wordbooks/system');
  },

  getUser() {
    return api.get<Wordbook[]>('/api/wordbooks/user');
  },

  create(data: CreateWordbookRequest) {
    return api.post<Wordbook>('/api/wordbooks', data);
  },

  getWords(id: string, params?: { page?: number; perPage?: number }) {
    return api.get<PaginatedResponse<Word>>(`/api/wordbooks/${id}/words`, params);
  },

  addWords(id: string, wordIds: string[]) {
    return api.post<{ added: number }>(`/api/wordbooks/${id}/words`, { wordIds });
  },

  removeWord(bookId: string, wordId: string) {
    return api.delete<{ removed: boolean }>(`/api/wordbooks/${bookId}/words/${wordId}`);
  },
};
