import { api } from './client';
import type { AuthResponse } from '@/types/user';

export const authApi = {
  refresh: () => api.post<AuthResponse>('/api/auth/refresh', undefined, { skipTokenRefresh: true }),
};
