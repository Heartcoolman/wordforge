import { api } from './client';
import type {
  AdminAuthResponse, AdminStats,
  AdminUsersPage, AdminUsersQuery,
  EngagementAnalytics, LearningAnalytics,
  SystemHealth, DatabaseInfo, SystemSettings,
  UpdateCheck, AdminUpdateStatus, DailyActiveUsersEntry, DailyRecordsEntry,
  StudyOverview, RecordTypeBreakdown, WordStateDistribution, RetentionCurve,
  FeedbackItem,
} from '@/types/admin';
import type { AmasConfig } from '@/types/amas';
import type { BrowseItem, WordbookPreview, ImportResult, UpdateInfo, SyncResult } from '@/types/wordbookCenter';

export type DataChannelValue = 'uploaded' | 'nil' | 'none';

export interface DataChannelStatus {
  amas: DataChannelValue;
  learning: DataChannelValue;
  telemetry: DataChannelValue;
}

export interface SseLiveEntry {
  deviceId: string;
  platform: string;
  userId: string;
  connectedSecs: number;
  connectionCount: number;
  isBanned: boolean;
  dataChannels: DataChannelStatus;
}

export interface RecentlyActiveEntry {
  deviceId: string;
  platform: string;
  userId: string | null;
  lastSeenAt: string;
  isBanned: boolean;
  dataChannels: DataChannelStatus;
}

export interface TelemetrySummary {
  id: string;
  deviceId: string;
  userId: string | null;
  eventType: string;
  serverTs: string;
  deviceProfile: {
    cpuCores: number | null;
    memoryGb: number | null;
    screenWidth: number | null;
    screenHeight: number | null;
    pixelRatio: number | null;
    osName: string | null;
    browserName: string | null;
    browserVersion: string | null;
    timezone: string | null;
    language: string | null;
    touchSupport: boolean | null;
    onlineStatus: boolean | null;
  };
  sessionStats: {
    sessionDurationSecs: number;
    actionsPerMin: number;
    errorCount: number;
    avgResponseTimeMs: number;
  };
  behaviorSummary: {
    currentRoute: string | null;
    clickCount: number | null;
    clickTargets: Array<{ label: string; tag: string }> | null;
    scrollDepthPct: number | null;
    visibilityChanges: number | null;
    routeChanges: number | null;
  };
  featureUsage: Record<string, number>;
}

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
  resetUserPassword: (id: string) => api.post<{ resetKey: string; expiresInHours: number }>(`/api/admin/users/${id}/reset-password`, undefined, { useAdminToken: true }),
  setUserPassword: (id: string, newPassword: string) => api.post<{ passwordReset: boolean; userId: string; sessionsRevoked: number }>(`/api/admin/users/${id}/set-password`, { newPassword }, { useAdminToken: true }),
  getStats: () => api.get<AdminStats>('/api/admin/stats', undefined, { useAdminToken: true }),

  // Analytics
  getEngagement: () => api.get<EngagementAnalytics>('/api/admin/analytics/engagement', undefined, { useAdminToken: true }),
  getLearningAnalytics: () => api.get<LearningAnalytics>('/api/admin/analytics/learning', undefined, { useAdminToken: true }),
  getDailyActiveUsers: (days?: number) =>
    api.get<DailyActiveUsersEntry[]>('/api/admin/analytics/daily-active-users', days ? { days } : undefined, { useAdminToken: true }),
  getDailyRecords: (days?: number) =>
    api.get<DailyRecordsEntry[]>('/api/admin/analytics/daily-records', days ? { days } : undefined, { useAdminToken: true }),
  getStudyOverview: (days?: number, category?: string) =>
    api.get<StudyOverview>('/api/admin/analytics/study-overview', { days, category }, { useAdminToken: true }),
  getRecordTypes: (days?: number) =>
    api.get<RecordTypeBreakdown>('/api/admin/analytics/record-types', days ? { days } : undefined, { useAdminToken: true }),
  getWordStateDistribution: (category?: string) =>
    api.get<WordStateDistribution>('/api/admin/analytics/word-states', category ? { category } : undefined, { useAdminToken: true }),
  getRetentionCurve: (category?: string) =>
    api.get<RetentionCurve>('/api/admin/analytics/retention-curve', category ? { category } : undefined, { useAdminToken: true }),

  // Monitoring
  getHealth: () => api.get<SystemHealth>('/api/admin/monitoring/health', undefined, { useAdminToken: true }),
  getDatabase: () => api.get<DatabaseInfo>('/api/admin/monitoring/database', undefined, { useAdminToken: true }),
  checkUpdate: () => api.get<UpdateCheck>('/api/admin/monitoring/check-update', undefined, { useAdminToken: true }),

  // 一键自更新（PR-auto-update）
  updatesStatus: () => api.get<AdminUpdateStatus>('/api/admin/updates/status', undefined, { useAdminToken: true }),
  updatesCheck: () => api.post<AdminUpdateStatus>('/api/admin/updates/check', undefined, { useAdminToken: true }),
  updatesApply: (targetVersion: string, confirmCurrentVersion: string) =>
    api.post<{ restarting: boolean }>(
      '/api/admin/updates/apply',
      { targetVersion, confirmCurrentVersion },
      { useAdminToken: true },
    ),

  // Broadcast & Settings
  broadcast: (data: { title: string; message: string }) => api.post<{ sent: number }>('/api/admin/broadcast', data, { useAdminToken: true }),
  getSettings: () => api.get<SystemSettings>('/api/admin/settings', undefined, { useAdminToken: true }),
  updateSettings: (data: Partial<SystemSettings>) => api.put<SystemSettings>('/api/admin/settings', data, { useAdminToken: true }),
  reloadAmas: (data: AmasConfig) => api.post<AmasConfig>('/api/admin/settings/reload-amas', data, { useAdminToken: true }),
  broadcastUpdate: (data?: { message?: string; version?: string }) =>
    api.post<{ broadcasted: boolean }>('/api/admin/broadcast-update', data || {}, { useAdminToken: true }),

  // Clients
  getClients: () =>
    api.get<{ sseLive: SseLiveEntry[]; recentlyActive: RecentlyActiveEntry[] }>('/api/admin/clients', undefined, { useAdminToken: true }),
  banClient: (id: string, reason?: string) =>
    api.post<{ banned: boolean; deviceId: string }>(`/api/admin/clients/${id}/ban`, reason ? { reason } : undefined, { useAdminToken: true }),
  unbanClient: (id: string) =>
    api.post<{ banned: boolean; deviceId: string }>(`/api/admin/clients/${id}/unban`, undefined, { useAdminToken: true }),
  requestTelemetry: (id: string) =>
    api.post<{ requestId: string }>(`/api/admin/clients/${id}/request-telemetry`, undefined, { useAdminToken: true }),
  getTelemetry: (deviceId: string, params?: { limit?: number; offset?: number }) =>
    api.get<{ records: TelemetrySummary[]; total: number }>(`/api/admin/telemetry/${deviceId}`, params as Record<string, string | number | boolean | undefined>, { useAdminToken: true }),

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

  // ─────────── 用户反馈（PR-feedback） ───────────
  listFeedback: (params?: { page?: number; perPage?: number }) =>
    api.get<{ items: FeedbackItem[]; total: number; page: number; perPage: number }>(
      '/api/admin/feedback',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),

  // ─────────── AMAS 配置版本（PR-2） ───────────
  amasListVersions: (limit = 50) =>
    api.get<AmasConfigVersionRow[]>('/api/admin/amas/config/versions', { limit }, { useAdminToken: true }),
  amasGetVersion: (hash: string) =>
    api.get<AmasConfigVersionDetail>(`/api/admin/amas/config/versions/${hash}`, undefined, { useAdminToken: true }),
  amasRestoreVersion: (hash: string, note?: string) =>
    api.post<{ updated: boolean; versionHash: string; versionId: number }>(
      `/api/admin/amas/config/versions/${hash}/restore`,
      { note },
      { useAdminToken: true },
    ),
  amasUpdateConfigWithNote: (config: AmasConfig, note?: string) =>
    api.put<{ updated: boolean; versionHash: string; versionId: number }>(
      `/api/admin/amas/config${note ? `?note=${encodeURIComponent(note)}` : ''}`,
      config,
      { useAdminToken: true },
    ),

  // ─────────── AMAS 可视化指标（PR-3） ───────────
  amasMetricsTimeseries: (days = 7) =>
    api.get<AmasMetricsTimeseriesPoint[]>('/api/admin/amas/metrics/timeseries', { days }, { useAdminToken: true }),
  amasAnomaliesOverview: (days = 7) =>
    api.get<AmasAnomalyOverview>('/api/admin/amas/anomalies', { days }, { useAdminToken: true }),
  amasUserStateDistribution: (days = 1, bins = 20) =>
    api.get<AmasUserStateDistribution>('/api/admin/amas/user-state/distribution', { days, bins }, { useAdminToken: true }),
  amasCompareVersions: (versionA: string, versionB: string) =>
    api.get<{ a: AmasVersionSlice; b: AmasVersionSlice }>('/api/admin/amas/compare', { versionA, versionB }, { useAdminToken: true }),

  // ─────────── AMAS Advisor / Suggestions（PR-5/6） ───────────
  amasListSuggestions: (status?: AmasSuggestionStatus, limit = 50) =>
    api.get<AmasSuggestion[]>('/api/admin/amas/suggestions', { status, limit }, { useAdminToken: true }),
  amasGetSuggestion: (id: number) =>
    api.get<AmasSuggestion>(`/api/admin/amas/suggestions/${id}`, undefined, { useAdminToken: true }),
  amasApproveSuggestion: (id: number, note?: string) =>
    api.post<{ updated: boolean; versionHash: string; versionId: number }>(`/api/admin/amas/suggestions/${id}/approve`, { note }, { useAdminToken: true }),
  amasRejectSuggestion: (id: number, note?: string) =>
    api.post<{ rejected: boolean }>(`/api/admin/amas/suggestions/${id}/reject`, { note }, { useAdminToken: true }),
  amasExplainParam: (path: string, currentValue: unknown) =>
    api.post<AmasExplainResponse>('/api/admin/amas/suggestions/explain', { path, currentValue }, { useAdminToken: true }),
  amasSuggestionSpend: () =>
    api.get<AmasSpendStats>('/api/admin/amas/suggestions/spend', undefined, { useAdminToken: true }),
};

// ─────────── 版本相关类型（PR-2） ───────────
export type AmasConfigVersionSource = 'manual' | 'llm_suggested' | 'llm_auto';

export interface AmasConfigVersionRow {
  id: number;
  versionHash: string;
  authorAdminId: string;
  source: AmasConfigVersionSource;
  note: string | null;
  parentVersionHash: string | null;
  createdAt: string;
}

export interface AmasConfigVersionDetail extends AmasConfigVersionRow {
  snapshotJson: Record<string, unknown>;
}

// ─────────── 可视化响应类型（PR-3） ───────────

export interface AmasMetricsTimeseriesPoint {
  date: string;
  algorithm: string;
  callCount: number;
  avgLatencyUs: number;
  errorCount: number;
}

export interface AmasDailyAnomaly {
  date: string;
  total: number;
  anomalies: number;
  violations: number;
}

export interface AmasViolationFieldStat {
  field: string;
  count: number;
}

export interface AmasAnomalyOverview {
  totalEvents: number;
  anomalyCount: number;
  violationCount: number;
  coldStartExplore: number;
  coldStartExploit: number;
  byDay: AmasDailyAnomaly[];
  topViolationFields: AmasViolationFieldStat[];
}

export interface AmasUserStateHistogram {
  field: string;
  min: number;
  max: number;
  bins: number[];
  mean: number;
  median: number;
  sampleSize: number;
}

export interface AmasUserStateDistribution {
  attention: AmasUserStateHistogram;
  fatigue: AmasUserStateHistogram;
  motivation: AmasUserStateHistogram;
  confidence: AmasUserStateHistogram;
  coldStartExplore: number;
  coldStartExploit: number;
}

export type AmasSuggestionStatus = 'pending' | 'approved' | 'rejected' | 'superseded' | 'expired' | 'auto_applied';

export interface AmasSuggestion {
  id: number;
  createdAt: string;
  basedOnVersionHash: string;
  patchJson: Record<string, number>;
  rationale: string;
  evidenceJson: Record<string, unknown>;
  status: AmasSuggestionStatus;
  decidedBy: string | null;
  decidedAt: string | null;
  decisionNote: string | null;
  costUsd: number | null;
  tokensInput: number | null;
  tokensOutput: number | null;
  confidence: number | null;
}

export interface AmasExplainResponse {
  explanation: string;
  model: string;
  costUsd: number;
  tokensInput: number;
  tokensOutput: number;
}

export interface AmasSpendStats {
  todayCostUsd: number;
  todayTokensInput: number;
  todayTokensOutput: number;
  dailyCapUsd: number;
  remainingUsd: number;
}

export interface AmasVersionSlice {
  versionHash: string;
  eventCount: number;
  anomalyCount: number;
  anomalyRate: number;
  meanLatencyMs: number;
  meanReward: number;
  meanAttention: number;
  meanFatigue: number;
  meanMotivation: number;
  meanConfidence: number;
  firstEventAt: string | null;
  lastEventAt: string | null;
}
