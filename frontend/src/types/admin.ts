export interface AdminUser {
  id: string;
  email: string;
  username: string;
  isBanned: boolean;
  createdAt: string;
}

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
  status: string;
  dbSizeBytes: number;
  uptime: number;
  version: string;
}

export interface DatabaseInfo {
  sizeOnDisk: number;
  treeCount: number;
  trees: Record<string, unknown>;
}

export interface SystemSettings {
  maxUsers: number;
  registrationEnabled: boolean;
  maintenanceMode: boolean;
  defaultDailyWords: number;
}
