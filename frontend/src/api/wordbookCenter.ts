import { api } from './client';
import type {
  BrowseItem,
  WordbookPreview,
  ImportResult,
  UpdateInfo,
  SyncResult,
  UserWbCenterSettings,
} from '@/types/wordbookCenter';

export const wordbookCenterApi = {
  // User endpoints
  getSettings: () =>
    api.get<UserWbCenterSettings>('/api/wordbook-center/settings'),

  updateSettings: (data: { wordbookCenterUrl: string | null }) =>
    api.put<UserWbCenterSettings>('/api/wordbook-center/settings', data),

  browse: () =>
    api.get<BrowseItem[]>('/api/wordbook-center/browse'),

  preview: (id: string, params?: { page?: number; perPage?: number }) =>
    api.get<WordbookPreview>(`/api/wordbook-center/browse/${id}`, params),

  import: (id: string) =>
    api.post<ImportResult>(`/api/wordbook-center/import/${id}`),

  importUrl: (url: string) =>
    api.post<ImportResult>('/api/wordbook-center/import-url', { url }),

  getUpdates: () =>
    api.get<UpdateInfo[]>('/api/wordbook-center/updates'),

  sync: (id: string) =>
    api.post<SyncResult>(`/api/wordbook-center/updates/${id}/sync`),
};
