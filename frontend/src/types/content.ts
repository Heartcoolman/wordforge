import type { Word } from './word';

export interface Etymology {
  wordId: string;
  word: string;
  etymology: string;
  roots: string[];
  generated: boolean;
}

export type MorphemeType = 'prefix' | 'root' | 'suffix';

export interface Morpheme {
  text: string;
  type: MorphemeType;
  meaning: string;
}

export interface WordContexts {
  wordId: string;
  word: string;
  examples: string[];
  contexts: string[];
}

// SemanticSearchItem 与 Word 字段完全相同，直接复用
export type SemanticSearchItem = Word;

export interface ConfusionPair {
  wordId: string;
  word: string;
  meaning: string;
  similarity: number;
}

export interface SemanticSearchResult {
  query: string;
  results: SemanticSearchItem[];
  total: number;
  method: string;
}

export interface ConfusionPairsResult {
  wordId: string;
  confusionPairs: ConfusionPair[];
}
