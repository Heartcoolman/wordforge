export interface Notification {
  id: string;
  userId: string;
  title: string;
  message: string;
  type: 'system' | 'achievement' | 'reminder' | 'info' | 'broadcast';
  read: boolean;
  createdAt: string;
}

export interface Badge {
  id: string;
  name: string;
  description: string;
  unlocked: boolean;
  progress: number;
  unlockedAt?: string;
}
