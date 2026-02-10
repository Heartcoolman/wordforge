import { api } from './client';
import type { Notification, Badge } from '@/types/notification';
import type { UserPreferences } from '@/types/user';

export const notificationsApi = {
  list: (params?: { limit?: number; unreadOnly?: boolean }) =>
    api.get<Notification[]>('/api/notifications', params),
  markRead: (id: string) =>
    api.put<{ read: boolean }>(`/api/notifications/${id}/read`),
  markAllRead: () =>
    api.post<{ markedRead: number }>('/api/notifications/read-all'),
  getBadges: () =>
    api.get<Badge[]>('/api/notifications/badges'),
  getPreferences: () =>
    api.get<UserPreferences>('/api/notifications/preferences'),
  updatePreferences: (data: Partial<UserPreferences>) =>
    api.put<UserPreferences>('/api/notifications/preferences', data),
};
