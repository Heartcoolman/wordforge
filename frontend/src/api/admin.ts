import { api } from './client';
import type {
  AdminAuthResponse, AdminStats,
  AdminUsersPage, AdminUsersQuery,
  EngagementAnalytics, LearningAnalytics,
  SystemHealth, DatabaseInfo, SystemSettings,
} from '@/types/admin';
import type { BrowseItem, WordbookPreview, ImportResult, UpdateInfo, SyncResult } from '@/types/wordbookCenter';

export const adminApi = {
  // Auth
  checkStatus: () => api.get<{ initialized: boolean }>('/api/admin/auth/status'),
  setup: (data: { email: string; password: string }) =>
    api.post<AdminAuthResponse>('/api/admin/auth/setup', data),
  login: (data: { email: string; password: string }) =>
    api.post<AdminAuthResponse>('/api/admin/auth/login', data),
  logout: () => api.post<{ loggedOut: boolean }>('/api/admin/auth/logout', undefined, { useAdminToken: true }),
  verifyToken: () => api.get<{ id: string; email: string }>('/api/admin/auth/verify', undefined, { useAdminToken: true }),

  // Users
  getUsers: (params?: AdminUsersQuery) =>
    api.get<AdminUsersPage>('/api/admin/users', params as Record<string, string | number | boolean | undefined>, { useAdminToken: true }),
  banUser: (id: string) => api.post<{ banned: boolean; userId: string }>(`/api/admin/users/${id}/ban`, undefined, { useAdminToken: true }),
  unbanUser: (id: string) => api.post<{ banned: boolean; userId: string }>(`/api/admin/users/${id}/unban`, undefined, { useAdminToken: true }),
  getStats: () => api.get<AdminStats>('/api/admin/stats', undefined, { useAdminToken: true }),

  // Analytics
  getEngagement: () => api.get<EngagementAnalytics>('/api/admin/analytics/engagement', undefined, { useAdminToken: true }),
  getLearningAnalytics: () => api.get<LearningAnalytics>('/api/admin/analytics/learning', undefined, { useAdminToken: true }),

  // Monitoring
  getHealth: () => api.get<SystemHealth>('/api/admin/monitoring/health', undefined, { useAdminToken: true }),
  getDatabase: () => api.get<DatabaseInfo>('/api/admin/monitoring/database', undefined, { useAdminToken: true }),

  // Broadcast & Settings
  broadcast: (data: { title: string; message: string }) => api.post<{ sent: number }>('/api/admin/broadcast', data, { useAdminToken: true }),
  getSettings: () => api.get<SystemSettings>('/api/admin/settings', undefined, { useAdminToken: true }),
  updateSettings: (data: Partial<SystemSettings>) => api.put<SystemSettings>('/api/admin/settings', data, { useAdminToken: true }),

  // Wordbook Center
  wbCenterBrowse: () =>
    api.get<BrowseItem[]>('/api/admin/wordbook-center/browse', undefined, { useAdminToken: true }),
  wbCenterPreview: (id: string, params?: { page?: number; perPage?: number }) =>
    api.get<WordbookPreview>(`/api/admin/wordbook-center/browse/${id}`, params as Record<string, string | number | boolean | undefined>, { useAdminToken: true }),
  wbCenterImport: (id: string) =>
    api.post<ImportResult>(`/api/admin/wordbook-center/import/${id}`, undefined, { useAdminToken: true }),
  wbCenterUpdates: () =>
    api.get<UpdateInfo[]>('/api/admin/wordbook-center/updates', undefined, { useAdminToken: true }),
  wbCenterSync: (id: string) =>
    api.post<SyncResult>(`/api/admin/wordbook-center/updates/${id}/sync`, undefined, { useAdminToken: true }),
};
