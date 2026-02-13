import type { PaginatedResponse } from './api';

export interface AdminUser {
  id: string;
  email: string;
  username: string;
  isBanned: boolean;
  failedLoginCount: number;
  lockedUntil: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AdminUsersQuery {
  page?: number;
  perPage?: number;
  search?: string;
  banned?: boolean;
}

export type AdminUsersPage = PaginatedResponse<AdminUser>;

export interface AdminAuthResponse {
  token: string;
  admin: { id: string; email: string };
}

export interface AdminStats {
  users: number;
  words: number;
  records: number;
}

export interface EngagementAnalytics {
  totalUsers: number;
  activeToday: number;
  retentionRate: number;
}

export interface LearningAnalytics {
  totalWords: number;
  totalRecords: number;
  totalCorrect: number;
  overallAccuracy: number;
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'down';
  dbSizeBytes: number;
  uptimeSecs: number;
  version: string;
}

export interface DatabaseInfo {
  sizeOnDisk: number;
  treeCount: number;
  trees: string[];
}

export interface SystemSettings {
  maxUsers: number;
  registrationEnabled: boolean;
  maintenanceMode: boolean;
  defaultDailyWords: number;
  wordbookCenterUrl?: string;
}
