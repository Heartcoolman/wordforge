export interface Wordbook {
  id: string;
  name: string;
  description: string;
  bookType: 'System' | 'User';
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
