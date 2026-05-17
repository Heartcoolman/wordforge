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

export interface TrendField {
  value: number;
  label: string;
}

export interface AdminStats {
  users: number;
  words: number;
  records: number;
  trend?: {
    users?: TrendField;
    records?: TrendField;
  };
}

export interface EngagementAnalytics {
  totalUsers: number;
  activeToday: number;
  retentionRate: number;
  trend?: {
    activeToday?: TrendField;
  };
}

export interface LearningAnalytics {
  totalWords: number;
  totalRecords: number;
  totalCorrect: number;
  overallAccuracy: number;
  trend?: {
    totalRecords?: TrendField;
    overallAccuracy?: TrendField;
  };
}

export interface SystemHealth {
  status: 'healthy' | 'degraded' | 'down';
  dbSizeBytes: number;
  uptimeSecs: number;
  version: string;
}

export interface DatabaseInfo {
  sizeOnDisk: number;
  tableCount: number;
  tables: string[];
  pageSize: number;
  pageCount: number;
  walEnabled: boolean;
}

export interface UpdateCheck {
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  releaseUrl: string | null;
  releaseNotes: string | null;
}

export interface SystemSettings {
  maxUsers: number;
  registrationEnabled: boolean;
  maintenanceMode: boolean;
  defaultDailyWords: number;
  wordbookCenterUrl?: string;
  amasAutoApplyEnabled: boolean;
  amasAutoApplyMaxPerDay: number;
  amasAutoApplyMinConfidence: number;
}

export interface DailyActiveUsersEntry {
  date: string;
  count: number;
  registered: number;
}

export interface DailyRecordsEntry {
  date: string;
  correct: number;
  total: number;
  durationSecs: number;
  newWords: number;
}

export interface StudyDailyEntry {
  date: string;
  durationSecs: number;
  sessionCount: number;
  recordCount: number;
  correctCount: number;
  accuracy: number | null;
  newWords: number;
  reviewWords: number;
  masteredWords: number;
}

export interface StudyOverview {
  generatedAt: string;
  days: number;
  category: string;
  summary: {
    totalDurationSecs: number;
    sessionCount: number;
    recordCount: number;
    correctCount: number;
    accuracy: number | null;
    newWords: number;
    reviewWords: number;
    masteredWords: number;
  };
  daily: StudyDailyEntry[];
}

export interface RecordTypeTotal {
  recordType: 'learning' | 'review' | 'all';
  total: number;
  correct: number;
  accuracy: number | null;
}

export interface RecordTypeDailyEntry {
  date: string;
  learning: number;
  review: number;
  all: number;
}

export interface RecordTypeBreakdown {
  generatedAt: string;
  days: number;
  totals: RecordTypeTotal[];
  daily: RecordTypeDailyEntry[];
}

export interface WordStateDistribution {
  generatedAt: string;
  category: string;
  states: {
    newCount: number;
    learning: number;
    reviewing: number;
    mastered: number;
    forgotten: number;
  };
  totals: {
    trackedWords: number;
    bookmarkedWords: number;
    dueReviewWords: number;
    overdueReviewWords: number;
    averageMasteryLevel: number | null;
  };
}

export interface RetentionPoint {
  daysSinceLearn: number;
  retention: number | null;
  sampleSize: number;
}

export interface RetentionCurve {
  generatedAt: string;
  category: string;
  points: RetentionPoint[];
  averageRetention: number | null;
}
