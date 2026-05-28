export interface Wordbook {
  id: string;
  name: string;
  description: string;
  type: 'system' | 'user';
  userId?: string;
  wordCount: number;
  createdAt: string;
}

export interface CreateWordbookRequest {
  name: string;
  description?: string;
}

export interface AddWordsToBookRequest {
  wordIds: string[];
}
