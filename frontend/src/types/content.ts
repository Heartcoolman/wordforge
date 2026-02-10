export interface Etymology {
  wordId: string;
  word: string;
  etymology: string;
  roots: string[];
  generated: boolean;
}

export interface Morpheme {
  text: string;
  type: string;
  meaning: string;
}

export interface WordContexts {
  wordId: string;
  word: string;
  examples: string[];
  contexts: string[];
}

export interface SemanticSearchItem {
  wordId: string;
  word: string;
  score: number;
  meaning: string;
}

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
