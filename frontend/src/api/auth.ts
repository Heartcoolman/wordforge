import { api } from './client';
import type { AuthResponse, LoginRequest, RegisterRequest } from '@/types/user';

export const authApi = {
  login: (data: LoginRequest) => api.post<AuthResponse>('/api/auth/login', data),
  register: (data: RegisterRequest) => api.post<AuthResponse>('/api/auth/register', data),
  refresh: () => api.post<AuthResponse>('/api/auth/refresh', undefined, { skipTokenRefresh: true }),
  logout: () => api.post<{ loggedOut: boolean }>('/api/auth/logout'),
  forgotPassword: (email: string) => api.post<{ success: boolean }>('/api/auth/forgot-password', { email }),
  resetPassword: (token: string, newPassword: string) => api.post<{ success: boolean }>('/api/auth/reset-password', { token, newPassword }),
};
