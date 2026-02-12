export interface Word {
  id: string;
  text: string;
  meaning: string;
  pronunciation?: string;
  partOfSpeech?: string;
  difficulty: number;
  examples: string[];
  tags: string[];
  createdAt: string;
}

export interface CreateWordRequest {
  text: string;
  meaning: string;
  pronunciation?: string;
  partOfSpeech?: string;
  difficulty?: number;
  examples?: string[];
  tags?: string[];
  id?: string;
}

export interface BatchCreateRequest {
  words: CreateWordRequest[];
}

export interface BatchCreateResponse {
  count: number;
  skipped: number[];
  items: Word[];
}

export interface ImportUrlRequest {
  url: string;
}

export interface ImportUrlResponse {
  imported: number;
  items: Word[];
}
