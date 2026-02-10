import { api } from './client';
import type { Etymology, Morpheme, WordContexts, SemanticSearchResult, ConfusionPairsResult } from '@/types/content';

export const contentApi = {
  getEtymology: (wordId: string) =>
    api.get<Etymology>(`/api/content/etymology/${wordId}`),
  semanticSearch: (query: string, limit = 10) =>
    api.get<SemanticSearchResult>('/api/content/semantic/search', { query, limit }),
  getWordContexts: (wordId: string) =>
    api.get<WordContexts>(`/api/content/word-contexts/${wordId}`),
  getMorphemes: (wordId: string) =>
    api.get<{ wordId: string; morphemes: Morpheme[] }>(`/api/content/morphemes/${wordId}`),
  getConfusionPairs: (wordId: string) =>
    api.get<ConfusionPairsResult>(`/api/content/confusion-pairs/${wordId}`),
};
