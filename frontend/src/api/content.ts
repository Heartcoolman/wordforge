import { api } from './client';
import type { Etymology, Morpheme, WordContexts, SemanticSearchResult, ConfusionPairsResult } from '@/types/content';
import { SEMANTIC_SEARCH_DEFAULT_LIMIT } from '@/lib/constants';

export const contentApi = {
  getEtymology: (wordId: string) =>
    api.get<Etymology>(`/api/content/etymology/${wordId}`),
  semanticSearch: (query: string, limit = SEMANTIC_SEARCH_DEFAULT_LIMIT) =>
    api.get<SemanticSearchResult>('/api/content/semantic/search', { query, limit }),
  getWordContexts: (wordId: string) =>
    api.get<WordContexts>(`/api/content/word-contexts/${wordId}`),
  getMorphemes: (wordId: string) =>
    api.get<{ wordId: string; morphemes: Morpheme[] }>(`/api/content/morphemes/${wordId}`),
  setMorphemes: (wordId: string, morphemes: Morpheme[]) =>
    api.post<{ wordId: string; morphemes: Morpheme[] }>(`/api/content/morphemes/${wordId}`, { morphemes }),
  getConfusionPairs: (wordId: string) =>
    api.get<ConfusionPairsResult>(`/api/content/confusion-pairs/${wordId}`),
};
