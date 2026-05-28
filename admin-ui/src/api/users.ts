import { api } from './client';
import type { User, UserStats, ChangePasswordRequest } from '@/types/user';

export const usersApi = {
  getMe() {
    return api.get<User>('/api/users/me');
  },

  updateMe(data: { username?: string }) {
    return api.put<User>('/api/users/me', data);
  },

  changePassword(data: ChangePasswordRequest) {
    return api.put<{ passwordChanged: boolean }>('/api/users/me/password', data);
  },

  getStats() {
    return api.get<UserStats>('/api/users/me/stats');
  },
};
