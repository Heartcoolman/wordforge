import { api, buildUrl } from './http';
import { tokenManager } from '@/lib/token';
import type {
  AdminAuthResponse, AdminStats,
  AdminUser, AdminUsersPage, AdminUsersQuery, AdminCreateUserPayload,
  ClientDeviceRow, AdminAuditEntry, UserActivityEntry, UserExtras,
  EngagementAnalytics, LearningAnalytics,
  SystemHealth, DatabaseInfo, SystemSettings, VersionGate, WorkerStatusRow, DeadLetterEntry,
  RequestMetrics, LogLine, AlertEvent,
  UpdateCheck, AdminUpdateStatus, ApplyAccepted, UpdateAuditEntry, DailyActiveUsersEntry, DailyRecordsEntry,
  ChangelogSummary, BackupList, BackupEntry, RollbackPreflight,
  StudyOverview, RecordTypeBreakdown, WordStateDistribution, RetentionCurve,
  FeedbackItem, FeedbackReply, FeedbackStats, FeedbackDetail,
  // m027:设备页对齐 clients.html 设计新增
  ListedDevice, ClientPlatformAgg, ClientVersionAgg, ClientUpgradePolicy,
  BroadcastStats, BroadcastHistoryEntry, ScheduledBroadcast, BackupTargetStatus,
} from '@/types/admin';
import type { AmasConfig } from '@/types/amas';
import type { BrowseItem, WordbookPreview, ImportResult, UpdateInfo, SyncResult } from '@/types/wordbookCenter';
import type {
  WordbookListResponse, WordbookStats, WordEntriesPage, WordbookHeatmap,
  WordbookUserDistribution, WordbookAuditPage, Wordbook, WordbookExport, WordEntryRow,
  AdminWordbooksQuery, AdminWordEntriesQuery, CreateWordbookPayload, UpdateWordbookPayload, AddWordEntryPayload,
} from '@/types/adminWordbooks';

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
  /** m022:`x-app-version` 头落库后透出;旧客户端未上报时为 null */
  appVersion?: string | null;
}

export interface RecentlyActiveEntry {
  deviceId: string;
  platform: string;
  userId: string | null;
  lastSeenAt: string;
  isBanned: boolean;
  dataChannels: DataChannelStatus;
  /** m022:同上 */
  appVersion?: string | null;
  /** m054:关联风控标记(共享 IP/账号/指纹被某次封禁牵连),用于列表红点提示 */
  riskFlag?: boolean;
  riskRelatedDevice?: string | null;
}

/** GET /api/admin/clients/flagged 返回的关联风控复核项(脱敏:不含 last_ip) */
export interface FlaggedClientEntry {
  deviceId: string;
  platform: string;
  userId: string | null;
  lastSeenAt: string;
  isBanned: boolean;
  appVersion: string | null;
  /** 打标原因(含触发源设备与命中信号) */
  riskReason: string | null;
  riskFlaggedAt: string | null;
  /** 触发本次标记的源被封设备 id */
  riskRelatedDevice: string | null;
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

/** 单 event_type 的全量聚合(遥测记录面板分类 chip + 每类聚合行) */
export interface TelemetryEventTypeStat {
  eventType: string;
  count: number;
  avgDurationSecs: number;
  totalErrors: number;
  avgActionsPerMin: number;
  avgResponseMs: number;
}

/** 名称→次数聚合项(功能使用 / 访问页面 / 点击热点) */
export interface NameCount {
  name: string;
  count: number;
}

/** GET /api/admin/telemetry/:device_id/summary 设备遥测分类总览 + 操作概览(全量聚合) */
export interface TelemetryDeviceSummary {
  total: number;
  firstTs: string | null;
  lastTs: string | null;
  byEventType: TelemetryEventTypeStat[];
  deviceProfile: TelemetrySummary['deviceProfile'] | null;
  featureUsage: NameCount[];
  routes: NameCount[];
  clickTargets: NameCount[];
  totalClicks: number;
  totalErrors: number;
  totalDurationSecs: number;
  sessionCount: number;
}

/** GET /api/admin/clients/:id 返回的单设备详情(脱敏:不含 last_ip / banned_by) */
export interface ClientDetail {
  deviceId: string;
  platform: string;
  userId: string | null;
  appVersion: string | null;
  /** m038:遥测硬识别上报的设备型号,未上报为 null */
  model: string | null;
  /** GeoIP 反查国家/地区码(无样本时 null) */
  country: string | null;
  firstSeenAt: string;
  lastSeenAt: string;
  isBanned: boolean;
  bannedAt: string | null;
  banReason: string | null;
  /** m054:关联风控标记(共享 IP/账号/指纹被牵连) */
  riskFlag: boolean;
  riskReason: string | null;
  riskFlaggedAt: string | null;
  riskRelatedDevice: string | null;
  /** 当前是否有活跃 SSE 连接 */
  online: boolean;
  /** 活跃 SSE 连接数(0 = 离线) */
  connectionCount: number;
  /** 近期遥测摘要:总条数 + 最近一条原始记录(latest 为 null 表示从未上报) */
  telemetry: { total: number; latest: TelemetrySummary | null };
}

export const adminApi = {
  // Auth
  checkStatus: () => api.get<{ initialized: boolean }>('/api/admin/auth/status'),
  setup: (data: { email: string; password: string }) =>
    api.post<AdminAuthResponse>('/api/admin/auth/setup', data),
  // be:login-health:rememberMe=true 签发 30 天 token(否则默认 2h)
  login: (data: { email: string; password: string; rememberMe?: boolean }) =>
    api.post<AdminAuthResponse>('/api/admin/auth/login', data),
  // be:setup-envcheck:登录前环境自检(公开,无需 admin token)
  setupEnvCheck: () =>
    api.get<EnvCheckResponse>('/api/admin/auth/setup/env-check'),
  // be:login-health:公开 health 端点(含 version / dbSizeBytes,供 footer)
  health: () => api.get<HealthInfo>('/health'),
  logout: () => api.post<{ loggedOut: boolean }>('/api/admin/auth/logout', undefined, { useAdminToken: true }),
  verifyToken: () => api.get<{ id: string; email: string }>('/api/admin/auth/verify', undefined, { useAdminToken: true }),

  // Users
  getUsers: (params?: AdminUsersQuery) =>
    api.get<AdminUsersPage>('/api/admin/users', params as Record<string, string | number | boolean | undefined>, { useAdminToken: true }),
  // m024:创建用户(admin 通道,跳过公共注册限制但仍走 email/username/password 校验)
  createUser: (payload: AdminCreateUserPayload) =>
    api.post<AdminUser>('/api/admin/users', payload, { useAdminToken: true }),
  banUser: (id: string) => api.post<{ banned: boolean; userId: string }>(`/api/admin/users/${id}/ban`, undefined, { useAdminToken: true }),
  unbanUser: (id: string) => api.post<{ banned: boolean; userId: string }>(`/api/admin/users/${id}/unban`, undefined, { useAdminToken: true }),
  resetUserPassword: (id: string) => api.post<{ resetKey: string; expiresInHours: number }>(`/api/admin/users/${id}/reset-password`, undefined, { useAdminToken: true }),
  setUserPassword: (id: string, newPassword: string) => api.post<{ passwordReset: boolean; userId: string; sessionsRevoked: number }>(`/api/admin/users/${id}/set-password`, { newPassword }, { useAdminToken: true }),
  // m024:角色变更(单条 + 批量)
  patchUserRole: (id: string, role: 'user' | 'staff' | 'admin') =>
    api.patch<AdminUser>(`/api/admin/users/${id}/role`, { role }, { useAdminToken: true }),
  usersBulkRole: (userIds: string[], role: 'user' | 'staff' | 'admin') =>
    api.post<BulkUserResult & { role: string }>('/api/admin/users/bulk-role', { userIds, role }, { useAdminToken: true }),
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
  /** m023:Dashboard worker 心跳网格数据源 */
  monitoringWorkers: () =>
    api.get<{ workers: WorkerStatusRow[] }>('/api/admin/monitoring/workers', undefined, { useAdminToken: true }),
  // m028:监控页对齐设计图新增——滚动请求指标 / 实时日志 / 派生告警时间线
  /** window: 15m | 1h | 6h | 24h | 7d */
  monitoringRequests: (window: '15m' | '1h' | '6h' | '24h' | '7d' = '1h') =>
    api.get<RequestMetrics>('/api/admin/monitoring/requests', { window }, { useAdminToken: true }),
  monitoringLogs: (limit = 200, level?: string) =>
    api.get<{ logs: LogLine[] }>('/api/admin/monitoring/logs', { limit, level }, { useAdminToken: true }),
  monitoringEvents: (hours = 6) =>
    api.get<{ events: AlertEvent[] }>('/api/admin/monitoring/events', { hours }, { useAdminToken: true }),
  // W1-2:outbox 死信运维——明细列表 + 人工重投 / 丢弃
  monitoringDeadLetter: (limit = 100) =>
    api.get<{ entries: DeadLetterEntry[] }>('/api/admin/monitoring/dead-letter', { limit }, { useAdminToken: true }),
  monitoringDeadLetterRequeue: (id: number) =>
    api.post<{ requeued: boolean; id: number }>(`/api/admin/monitoring/dead-letter/${id}/requeue`, undefined, { useAdminToken: true }),
  monitoringDeadLetterPurge: (id: number) =>
    api.delete<{ purged: boolean; id: number }>(`/api/admin/monitoring/dead-letter/${id}`, { useAdminToken: true }),

  // 一键自更新（PR-auto-update）
  updatesStatus: () => api.get<AdminUpdateStatus>('/api/admin/updates/status', undefined, { useAdminToken: true }),
  updatesCheck: () => api.post<AdminUpdateStatus>('/api/admin/updates/check', undefined, { useAdminToken: true }),
  // v0.5.2 起 apply 改为异步：返回 ApplyAccepted（含 taskId），前端通过 status 轮询拿进度。
  // v0.6.0-beta.3：必传 channel（"stable" | "beta"），后端用它定位 cache.<channel> 校验 target_version。
  updatesApply: (channel: 'stable' | 'beta', targetVersion: string, confirmCurrentVersion: string) =>
    api.post<ApplyAccepted>(
      '/api/admin/updates/apply',
      { channel, targetVersion, confirmCurrentVersion },
      { useAdminToken: true },
    ),
  // S5：升级历史审计列表
  updatesHistory: () =>
    api.get<{ entries: UpdateAuditEntry[] }>('/api/admin/updates/history', undefined, { useAdminToken: true }),
  // 真·任意版本回滚：后端跳过 semver 上升校验，用 down 链把当前库副本降级到目标版本 schema。
  updatesRollback: (channel: 'stable' | 'beta', targetVersion: string, confirmCurrentVersion: string) =>
    api.post<ApplyAccepted>(
      '/api/admin/updates/rollback',
      { channel, targetVersion, confirmCurrentVersion },
      { useAdminToken: true },
    ),
  // 回滚前预检（不下载、不执行）：能否回滚 / 目标-当前 schema / 将丢弃哪些新数据 / 是否需下载确认。
  updatesRollbackPreflight: (targetVersion: string, confirmCurrentVersion: string) =>
    api.post<RollbackPreflight>(
      '/api/admin/updates/rollback/preflight',
      { targetVersion, confirmCurrentVersion },
      { useAdminToken: true },
    ),
  // CHANGELOG（GitHub compare API 分类）。available=false 时前端回退 releaseNotes。
  updatesChangelog: (channel?: 'stable' | 'beta') =>
    api.get<ChangelogSummary>('/api/admin/updates/changelog', channel ? { channel } : undefined, { useAdminToken: true }),
  // SQLite 备份列表（升级备份 + 每日 / 手动 / 恢复前兜底）。
  updatesBackups: () =>
    api.get<BackupList>('/api/admin/updates/backups', undefined, { useAdminToken: true }),
  // 立即手动备份（VACUUM INTO / Online Backup）。
  updatesCreateBackup: () =>
    api.post<BackupEntry>('/api/admin/updates/backups', undefined, { useAdminToken: true }),
  // 从备份恢复（自动 pre-restore 兜底；恢复后建议重启）。
  updatesRestoreBackup: (name: string) =>
    api.post<{ restored: boolean; restartRecommended: boolean; preRestoreBackup: string }>(
      `/api/admin/updates/backups/${encodeURIComponent(name)}/restore`, undefined, { useAdminToken: true }),
  // 备份下载：Bearer 鉴权无法走 <a href>，故 fetch 取 Blob 生成 ObjectURL。
  updatesBackupDownloadUrl: async (name: string): Promise<string> => {
    const token = tokenManager.getAdminToken();
    const res = await fetch(buildUrl(`/api/admin/updates/backups/${encodeURIComponent(name)}/download`), {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!res.ok) throw new Error(`下载失败 (${res.status})`);
    return URL.createObjectURL(await res.blob());
  },

  // Broadcast & Settings
  /** 系统广播看板：近 30 天统计 + 历史列表（GET 与 POST 同路径）。
   *  be:broadcast-packs-minor:支持 offset/limit 分页,响应新增 pagination。
   *  E2:filter（all/week/failed）下推后端跨页生效;pagination.total 随 filter 变,KPI 卡 stats 不变。 */
  listBroadcasts: (params?: { offset?: number; limit?: number; filter?: 'all' | 'week' | 'failed' }) =>
    api.get<{ stats: BroadcastStats; broadcasts: BroadcastHistoryEntry[]; pagination: BroadcastPagination }>(
      '/api/admin/broadcast',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  broadcast: (data: {
    title: string;
    message: string;
    /** m027:可选受众条件,任一字段为空数组 / null 都视为该维度不过滤;整个 audience 为 undefined 时走全员广播。 */
    audience?: {
      platforms?: string[];
      versionMin?: string | null;
      lastActiveDays?: number | null;
      userIds?: string[];
    };
    /** m042/D2:投递时机。未来 RFC3339 时间→入定时队列(返回 scheduled:true);省略/过去→立即下发。 */
    scheduledAt?: string;
  }) => api.post<{ sent?: number; scheduled?: boolean; scheduledAt?: string; broadcastId: string }>('/api/admin/broadcast', data, { useAdminToken: true }),
  /** m027:广播受众预估命中(不持久化),供 DevicesPage 嵌入 broadcast section "X / N 用户" 显示 */
  broadcastPreview: (data: {
    audience?: {
      platforms?: string[];
      versionMin?: string | null;
      lastActiveDays?: number | null;
      userIds?: string[];
    };
  }) => api.post<{ matched: number; total: number }>('/api/admin/broadcast/preview', data, { useAdminToken: true }),
  /** m042/D2:推送编辑器「保存草稿」全局单份(存/取/删) */
  getPushDraft: () =>
    api.get<{ draft: PushDraft | null }>('/api/admin/broadcast/draft', undefined, { useAdminToken: true }),
  savePushDraft: (data: {
    title: string;
    message: string;
    platforms?: string[];
    versionMin?: string | null;
    lastActiveDays?: number | null;
  }) => api.post<{ draft: PushDraft }>('/api/admin/broadcast/draft', data, { useAdminToken: true }),
  deletePushDraft: () =>
    api.delete<{ deleted: boolean }>('/api/admin/broadcast/draft', { useAdminToken: true }),
  // W2-2:定时广播队列——列出待发 + 按 id 取消
  listScheduledBroadcasts: () =>
    api.get<{ items: ScheduledBroadcast[] }>('/api/admin/broadcast/scheduled', undefined, { useAdminToken: true }),
  cancelScheduledBroadcast: (id: string) =>
    api.delete<{ canceled: boolean }>(`/api/admin/broadcast/scheduled/${encodeURIComponent(id)}`, { useAdminToken: true }),
  // W2-4:离站备份各 target 运行时状态（只读）
  getBackupStatus: () =>
    api.get<{ targets: BackupTargetStatus[] }>('/api/admin/settings/backup-status', undefined, { useAdminToken: true }),
  getSettings: () => api.get<SystemSettings>('/api/admin/settings', undefined, { useAdminToken: true }),
  updateSettings: (data: Partial<SystemSettings>) => api.put<SystemSettings>('/api/admin/settings', data, { useAdminToken: true }),
  setMaintenance: (active: boolean) => api.post<{ active: boolean }>('/api/admin/settings/maintenance', { active }, { useAdminToken: true }),
  // D4:客户端最低版本门控运行时读写(即时生效,strict-mode 发布切流)
  getVersionGate: () => api.get<VersionGate>('/api/admin/settings/version-gate', undefined, { useAdminToken: true }),
  setVersionGate: (data: { enabled?: boolean; minClientVersion?: string | null }) =>
    api.put<VersionGate>('/api/admin/settings/version-gate', data, { useAdminToken: true }),
  reloadAmas: (data: AmasConfig) => api.post<AmasConfig>('/api/admin/settings/reload-amas', data, { useAdminToken: true }),
  broadcastUpdate: (data?: { message?: string; version?: string }) =>
    api.post<{ broadcasted: boolean }>('/api/admin/broadcast-update', data || {}, { useAdminToken: true }),

  // Clients
  getClients: () =>
    api.get<{ sseLive: SseLiveEntry[]; recentlyActive: RecentlyActiveEntry[] }>('/api/admin/clients', undefined, { useAdminToken: true }),
  banClient: (id: string, reason?: string) =>
    api.post<{ banned: boolean; deviceId: string; flaggedRelated: number; flaggedDeviceIds: string[] }>(`/api/admin/clients/${id}/ban`, reason ? { reason } : undefined, { useAdminToken: true }),
  unbanClient: (id: string) =>
    api.post<{ banned: boolean; deviceId: string; clearedRelated: number }>(`/api/admin/clients/${id}/unban`, undefined, { useAdminToken: true }),
  // m054:关联风控复核——列被标记设备 + 清除单设备误报标记
  listFlaggedClients: () =>
    api.get<{ flagged: FlaggedClientEntry[] }>('/api/admin/clients/flagged', undefined, { useAdminToken: true }),
  clearClientFlag: (id: string) =>
    api.post<{ cleared: boolean; deviceId: string }>(`/api/admin/clients/${id}/clear-flag`, undefined, { useAdminToken: true }),
  requestTelemetry: (id: string) =>
    api.post<{ requestId: string }>(`/api/admin/clients/${id}/request-telemetry`, undefined, { useAdminToken: true }),
  getTelemetry: (deviceId: string, params?: { limit?: number; offset?: number; eventType?: string }) =>
    api.get<{ records: TelemetrySummary[]; total: number }>(`/api/admin/telemetry/${deviceId}`, params as Record<string, string | number | boolean | undefined>, { useAdminToken: true }),
  // 设备遥测分类总览(全量计数 + 每类聚合 + 设备画像),供"遥测记录"面板分类 chip
  getTelemetrySummary: (deviceId: string) =>
    api.get<TelemetryDeviceSummary>(`/api/admin/telemetry/${deviceId}/summary`, undefined, { useAdminToken: true }),
  // be:client-detail:单设备详情抽屉数据源(country/online/firstSeenAt/ban 元数据 + 遥测摘要)
  getClientDetail: (id: string) =>
    api.get<ClientDetail>(`/api/admin/clients/${id}`, undefined, { useAdminToken: true }),

  // m027:设备表后端分页 + 平台聚合 + 升级策略 CRUD + 强制升级广播
  getClientsPaginated: (params?: {
    page?: number; perPage?: number; q?: string; platform?: string; recentMinutes?: number;
  }) =>
    api.get<{ data: ListedDevice[]; total: number; page: number; perPage: number; totalPages: number }>(
      '/api/admin/clients/paginated',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  getClientsDistribution: () =>
    api.get<{
      platforms: ClientPlatformAgg[];
      versions: ClientVersionAgg[];
      policies: ClientUpgradePolicy[];
    }>('/api/admin/clients/distribution', undefined, { useAdminToken: true }),
  putUpgradePolicy: (platform: string, payload: {
    minVersion?: string | null; suggestedVersion?: string | null;
    grayscalePct?: number; pwaSilentUpdate?: boolean;
  }) =>
    api.put<{ ok: boolean; platform: string }>(`/api/admin/clients/upgrade-policy/${platform}`, payload, { useAdminToken: true }),
  broadcastUpgrade: (platform: string, payload: {
    belowVersion: string; latestVersion: string; message?: string;
  }) =>
    api.post<{ matched: number; pushedConnections: number }>(`/api/admin/clients/broadcast-upgrade/${platform}`, payload, { useAdminToken: true }),

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

  // ─────────── 词库中心:本地系统+用户词库管理(/api/admin/wordbooks/*) ───────────
  adminWordbooksList: (params?: AdminWordbooksQuery) =>
    api.get<WordbookListResponse>(
      '/api/admin/wordbooks',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  adminWordbookStats: (id: string) =>
    api.get<WordbookStats>(`/api/admin/wordbooks/${id}/stats`, undefined, { useAdminToken: true }),
  adminWordbookWords: (id: string, params?: AdminWordEntriesQuery) =>
    api.get<WordEntriesPage>(
      `/api/admin/wordbooks/${id}/words`,
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  adminWordbookHeatmap: (id: string, limit = 600) =>
    api.get<WordbookHeatmap>(`/api/admin/wordbooks/${id}/heatmap`, { limit }, { useAdminToken: true }),
  adminWordbookDistribution: (id: string) =>
    api.get<WordbookUserDistribution>(`/api/admin/wordbooks/${id}/user-distribution`, undefined, { useAdminToken: true }),
  adminWordbookHistory: (id: string, params?: { page?: number; perPage?: number }) =>
    api.get<WordbookAuditPage>(
      `/api/admin/wordbooks/${id}/history`,
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  adminWordbookCreate: (payload: CreateWordbookPayload) =>
    api.post<Wordbook>('/api/admin/wordbooks', payload, { useAdminToken: true }),
  adminWordbookUpdate: (id: string, payload: UpdateWordbookPayload) =>
    api.patch<Wordbook>(`/api/admin/wordbooks/${id}`, payload, { useAdminToken: true }),
  adminWordbookDelete: (id: string) =>
    api.delete<{ deleted: boolean }>(`/api/admin/wordbooks/${id}`, { useAdminToken: true }),
  adminWordbookAddWord: (id: string, payload: AddWordEntryPayload) =>
    api.post<WordEntryRow>(`/api/admin/wordbooks/${id}/words`, payload, { useAdminToken: true }),
  adminWordbookRemoveWord: (id: string, wordId: string) =>
    api.delete<{ removed: boolean }>(`/api/admin/wordbooks/${id}/words/${wordId}`, { useAdminToken: true }),
  adminWordbookExport: (id: string) =>
    api.get<WordbookExport>(`/api/admin/wordbooks/${id}/export`, undefined, { useAdminToken: true }),

  // ─────────── 用户反馈（PR-feedback） ───────────
  // 后端 paginated() 返回 {success, data:{ data, total, page, perPage, totalPages }}，
  // client.ts unwrap 剥外层 success/data 后，列表字段是 `data` 而非 `items`
  // —— v0.6.0-beta.1 这里曾错写 items，导致 FeedbackPage 渲染期 undefined.length 触发 ErrorBoundary。
  listFeedback: (params?: {
    page?: number;
    perPage?: number;
    category?: string;
    status?: string;
    /** m030:仅未读(readAt IS NULL) */
    unread?: boolean;
    /** m030:仅已分派(assigneeAdminId IS NOT NULL) */
    assigned?: boolean;
  }) =>
    api.get<{ data: FeedbackItem[]; total: number; page: number; perPage: number; totalPages: number }>(
      '/api/admin/feedback',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  /** PATCH /api/admin/feedback/:id — 更新优先级 / 状态 / 处理人 / 解决备注。
   * assigneeAdminId 传 null 等于"取消指派";不传字段表示"不改动"。 */
  updateFeedback: (
    id: string,
    payload: {
      priority?: string;
      status?: string;
      assigneeAdminId?: string | null;
      resolution?: string | null;
    },
  ) =>
    api.patch<FeedbackItem>(`/api/admin/feedback/${id}`, payload, { useAdminToken: true }),

  // ─────────── 反馈中心工单化（m030） ───────────
  /** GET /api/admin/feedback/stats — 反馈中心 KPI 聚合 */
  getFeedbackStats: () =>
    api.get<FeedbackStats>('/api/admin/feedback/stats', undefined, { useAdminToken: true }),
  /** GET /api/admin/feedback/:id — 工单详情(条目+回复+时间线+附件) */
  getFeedbackDetail: (id: string) =>
    api.get<FeedbackDetail>(`/api/admin/feedback/${id}`, undefined, { useAdminToken: true }),
  /** POST /api/admin/feedback/:id/replies — 回复用户;pushInapp 发应用内通知 */
  createFeedbackReply: (
    id: string,
    payload: { body: string; pushInapp?: boolean },
  ) => api.post<FeedbackReply>(`/api/admin/feedback/${id}/replies`, payload, { useAdminToken: true }),
  /** POST /api/admin/feedback/:id/assign — 分派给指定 admin,传 null 取消分派 */
  assignFeedback: (id: string, assigneeAdminId: string | null) =>
    api.post<FeedbackItem>(`/api/admin/feedback/${id}/assign`, { assigneeAdminId }, { useAdminToken: true }),
  /** POST /api/admin/feedback/:id/resolve — 标记已解决 */
  resolveFeedback: (id: string, resolution?: string | null) =>
    api.post<FeedbackItem>(`/api/admin/feedback/${id}/resolve`, { resolution }, { useAdminToken: true }),
  /** POST /api/admin/feedback/:id/merge — 合并到目标工单(本工单标 closed,目标 dedupCount+1) */
  mergeFeedback: (id: string, targetId: string) =>
    api.post<{ merged: boolean }>(`/api/admin/feedback/${id}/merge`, { targetId }, { useAdminToken: true }),
  /** POST /api/admin/feedback/:id/github-issue — 转 GitHub Issue;未配置返回 409 GITHUB_NOT_CONFIGURED */
  createFeedbackGithubIssue: (id: string) =>
    api.post<{ issueUrl: string }>(`/api/admin/feedback/${id}/github-issue`, {}, { useAdminToken: true }),
  /** POST /api/admin/feedback/mark-all-read — 全部标记已读 */
  markAllFeedbackRead: () =>
    api.post<{ updated: number }>('/api/admin/feedback/mark-all-read', {}, { useAdminToken: true }),
  /**
   * GET /api/admin/feedback/export.csv — 导出 CSV。
   * 端点走 Bearer 鉴权,浏览器直接打开 URL 无法携带 admin token,
   * 故 fetch 带头取回正文生成 Blob ObjectURL,页面用 <a download> 触发后 revoke。
   */
  feedbackCsvUrl: async (): Promise<string> => {
    const token = tokenManager.getAdminToken();
    const res = await fetch(buildUrl('/api/admin/feedback/export.csv'), {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!res.ok) throw new Error(`导出失败 (${res.status})`);
    const blob = await res.blob();
    return URL.createObjectURL(blob);
  },

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
  amasMetricsKpi: (days = 7) =>
    api.get<AmasMetricsKpi>('/api/admin/amas/metrics/kpi', { days }, { useAdminToken: true }),
  amasAlgorithmDistribution: (days = 7) =>
    api.get<AmasAlgorithmShare[]>('/api/admin/amas/metrics/algorithm-distribution', { days }, { useAdminToken: true }),
  amasAnomaliesOverview: (days = 7) =>
    api.get<AmasAnomalyOverview>('/api/admin/amas/anomalies', { days }, { useAdminToken: true }),
  amasUserStateDistribution: (days = 1, bins = 20) =>
    api.get<AmasUserStateDistribution>('/api/admin/amas/user-state/distribution', { days, bins }, { useAdminToken: true }),
  amasCompareVersions: (versionA: string, versionB: string) =>
    api.get<{ a: AmasVersionSlice; b: AmasVersionSlice }>('/api/admin/amas/compare', { versionA, versionB }, { useAdminToken: true }),

  // ─────────── AMAS 看板对齐设计稿新增端点（真实聚合） ───────────
  amasStageDistribution: () =>
    api.get<AmasStageDistribution>('/api/admin/amas/metrics/stage-distribution', undefined, { useAdminToken: true }),
  amasEloScatter: (limit = 400) =>
    api.get<AmasEloScatter>('/api/admin/amas/metrics/elo-scatter', { limit }, { useAdminToken: true }),
  amasMdmHeatmap: (days = 7) =>
    api.get<AmasMdmHeatmap>('/api/admin/amas/metrics/mdm-heatmap', { days }, { useAdminToken: true }),
  amasFatigueTimeseries: (days = 7) =>
    api.get<AmasFatigueTimeseries>('/api/admin/amas/metrics/fatigue-timeseries', { days }, { useAdminToken: true }),
  amasDecisionHistogram: (days = 7) =>
    api.get<AmasDecisionHistogram>('/api/admin/amas/metrics/decision-histogram', { days }, { useAdminToken: true }),
  amasStateTransitions: (hours = 24) =>
    api.get<AmasStateTransitions>('/api/admin/amas/user-state/transitions', { hours }, { useAdminToken: true }),
  amasLearningClusters: () =>
    api.get<AmasLearningStyleClusters>('/api/admin/amas/user-state/clusters', undefined, { useAdminToken: true }),
  amasAnomalyFeed: (days = 7, limit = 50) =>
    api.get<AmasAnomalyFeed>('/api/admin/amas/anomalies/feed', { days, limit }, { useAdminToken: true }),
  amasCompareVersionsExt: (versionA: string, versionB: string) =>
    api.get<{ a: AmasVersionSliceExt; b: AmasVersionSliceExt }>('/api/admin/amas/compare/ext', { versionA, versionB }, { useAdminToken: true }),

  // ─────────── AMAS Advisor / Suggestions（PR-5/6） ───────────
  amasListSuggestions: (status?: AmasSuggestionStatus, limit = 50, offset = 0, q?: string) =>
    api.get<AmasSuggestion[]>('/api/admin/amas/suggestions', { status, limit, offset, q }, { useAdminToken: true }),
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

  // ─────────── advisor 成本/统计/巡查（C1-C2） ───────────
  amasAdvisorCost: () =>
    api.get<AdvisorCostStats>('/api/admin/amas/advisor/cost', undefined, { useAdminToken: true }),
  amasAdvisorCostDaily: (days = 30) =>
    api.get<AdvisorCostDaily[]>('/api/admin/amas/advisor/cost/daily', { days }, { useAdminToken: true }),
  amasAdvisorRun: () =>
    api.post<{ produced: boolean; suggestionId: number | null }>('/api/admin/amas/advisor/run', undefined, { useAdminToken: true }),
  amasApproveAllSuggestions: () =>
    api.post<{ results: Array<{ id: number; ok: boolean; error: string | null }> }>(
      '/api/admin/amas/suggestions/approve-all', undefined, { useAdminToken: true }),

  // ─────────── advisor 配置（C3） ───────────
  amasAdvisorConfig: () =>
    api.get<AdvisorConfig>('/api/admin/amas/advisor/config', undefined, { useAdminToken: true }),
  amasUpdateAdvisorConfig: (payload: Partial<Pick<AdvisorConfig,
    'monthCapYuan' | 'autoApplyEnabled' | 'autoApplyMaxPerDay' | 'autoApplyMinConfidence' | 'grayscaleSteps' | 'advisorEnabled'>>) =>
    api.put<AdvisorConfig>('/api/admin/amas/advisor/config', payload, { useAdminToken: true }),

  // ─────────── 白名单 CRUD（C4） ───────────
  amasListWhitelist: () =>
    api.get<WhitelistRow[]>('/api/admin/amas/advisor/whitelist', undefined, { useAdminToken: true }),
  amasAddWhitelist: (payload: { path: string; minSafe: number; maxSafe: number }) =>
    api.post<WhitelistRow>('/api/admin/amas/advisor/whitelist', payload, { useAdminToken: true }),
  amasDeleteWhitelist: (path: string) =>
    api.delete<{ deleted: boolean }>(`/api/admin/amas/advisor/whitelist/${encodeURIComponent(path)}`, { useAdminToken: true }),

  // ─────────── 历史增强（C5） ───────────
  amasRollbackSuggestion: (id: number) =>
    api.post<{ rolledBack: boolean; versionHash: string }>(`/api/admin/amas/suggestions/${id}/rollback`, undefined, { useAdminToken: true }),
  /** CSV 导出走原始 fetch（响应是 text/csv 非 JSON envelope，api.get 的 unwrap 会误解析）。 */
  amasExportSuggestionsCsv: async (status?: AmasSuggestionStatus, q?: string): Promise<string> => {
    const url = buildUrl('/api/admin/amas/suggestions/export.csv', { status, q });
    const token = tokenManager.getAdminToken();
    const res = await fetch(url, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      credentials: 'include',
    });
    if (!res.ok) throw new Error(`导出失败 HTTP ${res.status}`);
    return res.text();
  },

  // ─────────── per-patch canary（C6） ───────────
  amasListCanaries: () =>
    api.get<PatchCanaryWithMetrics[]>('/api/admin/amas/advisor/canary', undefined, { useAdminToken: true }),
  amasCreateCanary: (payload: { suggestionId: number; percent: number }) =>
    api.post<PatchCanary>('/api/admin/amas/advisor/canary', payload, { useAdminToken: true }),
  amasScaleCanary: (id: number, percent: number) =>
    api.post<PatchCanary>(`/api/admin/amas/advisor/canary/${id}/scale`, { percent }, { useAdminToken: true }),
  amasRollbackCanary: (id: number) =>
    api.post<{ rolledBack: boolean }>(`/api/admin/amas/advisor/canary/${id}/rollback`, undefined, { useAdminToken: true }),
  amasPromoteCanary: (id: number) =>
    api.post<{ promoted: boolean; versionHash: string }>(`/api/admin/amas/advisor/canary/${id}/promote`, undefined, { useAdminToken: true }),

  // ─────────── T1.3：真实留存 A/B 实验 ───────────
  amasListExperiments: (status?: ExperimentStatus) =>
    api.get<AmasExperiment[]>('/api/admin/amas/experiments', status ? { status } : undefined, { useAdminToken: true }),
  amasRegisterExperiment: (payload: RegisterExperimentPayload) =>
    api.post<AmasExperiment>('/api/admin/amas/experiments', payload, { useAdminToken: true }),
  amasExperimentMetrics: (id: string) =>
    api.get<AmasExperimentMetricsResponse>(`/api/admin/amas/experiments/${encodeURIComponent(id)}/metrics`, undefined, { useAdminToken: true }),
  amasConcludeExperiment: (id: string, adopt: boolean) =>
    api.post<{ experimentId: string; status: string }>(`/api/admin/amas/experiments/${encodeURIComponent(id)}/conclude`, { adopt }, { useAdminToken: true }),
  amasPlanExperiment: (payload: PlanExperimentPayload) =>
    api.post<AmasExperimentPlan>('/api/admin/amas/experiments/plan', payload, { useAdminToken: true }),

  // ─────────── m022:新增端点全集 ───────────
  // AMAS TOML 互转
  amasParseToml: (toml: string) =>
    api.post<AmasConfig>('/api/admin/amas/config/parse-toml', { toml }, { useAdminToken: true }),
  amasSerializeToml: (config: AmasConfig) =>
    api.post<{ toml: string }>('/api/admin/amas/config/serialize-toml', config, { useAdminToken: true }),

  // AMAS canary 灰度
  amasGetCanary: () =>
    api.get<{ canary: AmasCanaryConfig | null }>('/api/admin/amas/config/canary', undefined, { useAdminToken: true }),
  amasSetCanary: (payload: { versionHash: string; percent: number; forceUserIds?: string[] }) =>
    api.put<{ canary: AmasCanaryConfig }>('/api/admin/amas/config/canary', payload, { useAdminToken: true }),
  amasDisableCanary: () =>
    api.post<{ disabled: boolean }>('/api/admin/amas/config/canary/disable', undefined, { useAdminToken: true }),

  // Wordbook 本地上传 + 标签覆盖
  wbCenterUpload: (payload: {
    id: string;
    name: string;
    description?: string;
    version?: string;
    tags?: string[];
    words: Array<{ spelling: string; phonetic?: string; meanings?: string[]; examples?: string[] }>;
  }) => api.post<ImportResult>('/api/admin/wordbook-center/upload', payload, { useAdminToken: true }),
  wbCenterPatchTags: (
    wordbookId: string,
    payload: { add?: string[]; remove?: string[]; replace?: string[] },
  ) =>
    api.patch<{ tags: string[] }>(`/api/admin/wordbook-center/${wordbookId}/tags`, payload, {
      useAdminToken: true,
    }),

  // Analytics 三新端点
  analyticsHourly: (days = 7) =>
    api.get<{ generatedAt: string; days: number; matrix: number[][]; total: number }>(
      '/api/admin/analytics/hourly',
      { days },
      { useAdminToken: true },
    ),
  analyticsWordbookRank: (params?: { days?: number; limit?: number }) =>
    api.get<{ generatedAt: string; days: number; limit: number; rows: WordbookRankRow[] }>(
      '/api/admin/analytics/wordbook-rank',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  analyticsRetentionCohort: (params?: { cohort?: 'weekly' | 'daily'; maxDays?: number }) =>
    api.get<{
      generatedAt: string;
      cohortUnit: string;
      maxDays: number;
      rows: Array<{ cohortStart: string; daysSince: number; retainedUsers: number }>;
    }>('/api/admin/analytics/retention-cohort', params as Record<string, string | number | boolean | undefined>, {
      useAdminToken: true,
    }),

  // Analytics 看板深化（KPI / 漏斗 / cohort 矩阵 / 题型分布 / 高频词 / 洞察）
  // 窗口型端点：days ∈ 7|14|30|90（默认 7），可选 from/to(YYYY-MM-DD) 覆盖 days（from/to 都给时优先）。
  analyticsKpiSummary: (params?: { days?: number; from?: string; to?: string }) =>
    api.get<AnalyticsKpiSummary>(
      '/api/admin/analytics/kpi-summary',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  analyticsFunnel: (params?: { days?: number; from?: string; to?: string }) =>
    api.get<AnalyticsFunnel>(
      '/api/admin/analytics/funnel',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  analyticsRetentionMatrix: (weeks = 7) =>
    api.get<AnalyticsRetentionMatrix>(
      '/api/admin/analytics/retention-matrix',
      { weeks },
      { useAdminToken: true },
    ),
  analyticsQuestionDistribution: (params?: { days?: number; from?: string; to?: string }) =>
    api.get<AnalyticsQuestionDistribution>(
      '/api/admin/analytics/question-distribution',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  analyticsWordFrequency: (params?: {
    days?: number; limit?: number; sort?: 'count' | 'accuracy' | 'elo' | 'mastery'; from?: string; to?: string;
  }) =>
    api.get<AnalyticsWordFrequency>(
      '/api/admin/analytics/word-frequency',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  analyticsInsights: (params?: { days?: number; from?: string; to?: string }) =>
    api.get<AnalyticsInsights>(
      '/api/admin/analytics/insights',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),

  // Users 扩展
  userProfile: (id: string) =>
    api.get<UserProfile>(`/api/admin/users/${id}/profile`, undefined, { useAdminToken: true }),
  userSessions: (id: string, limit = 20) =>
    api.get<{ sessions: UserSessionRow[] }>(`/api/admin/users/${id}/sessions`, { limit }, { useAdminToken: true }),
  usersBulkBan: (userIds: string[], reason?: string) =>
    api.post<BulkUserResult>('/api/admin/users/bulk-ban', { userIds, reason }, { useAdminToken: true }),
  usersBulkUnban: (userIds: string[]) =>
    api.post<BulkUserResult>('/api/admin/users/bulk-unban', { userIds }, { useAdminToken: true }),
  // m024:Drawer 数据源 —— 设备会话(复用 client_devices) + admin 操作审计
  userDevices: (id: string, limit = 20) =>
    api.get<{ devices: ClientDeviceRow[] }>(`/api/admin/users/${id}/devices`, { limit }, { useAdminToken: true }),
  userAuditLog: (id: string, limit = 50) =>
    api.get<{ entries: AdminAuditEntry[] }>(`/api/admin/users/${id}/audit-log`, { limit }, { useAdminToken: true }),
  // m025:用户档案深化(preferences/elo/habit/streak/latest strategy/latest device/7d metrics)
  userExtras: (id: string) =>
    api.get<UserExtras>(`/api/admin/users/${id}/extras`, undefined, { useAdminToken: true }),
  // m025:用户自有活动日志(login / session.complete / goal.update / fatigue.alert)
  userActivityLog: (id: string, limit = 50) =>
    api.get<{ entries: UserActivityEntry[] }>(`/api/admin/users/${id}/activity-log`, { limit }, { useAdminToken: true }),
  // m026:批量重置密码(密钥不回写,前端单查显示)
  usersBulkResetPassword: (userIds: string[]) =>
    api.post<BulkUserResult>('/api/admin/users/bulk-reset-password', { userIds }, { useAdminToken: true }),
  // m026:批量删除(危险,带 reason 强 audit)
  usersBulkDelete: (userIds: string[], reason: string) =>
    api.post<BulkUserResult>('/api/admin/users/bulk-delete', { userIds, reason }, { useAdminToken: true }),
  // m026:设备封禁(踢 user 所有会话兜底)
  banUserDevice: (userId: string, deviceId: string) =>
    api.post<{ banned: boolean; deviceId: string }>(
      `/api/admin/users/${userId}/devices/${deviceId}/ban`,
      undefined,
      { useAdminToken: true },
    ),

  // ═══════════ be:analytics-misc:用户筛选 chip 计数 ═══════════
  userFacets: () =>
    api.get<UserFacets>('/api/admin/users/facets', undefined, { useAdminToken: true }),

  // ═══════════ be:settings-*:通用配置存储 ═══════════
  /** GET /config — 全部 section(密钥已遮蔽) */
  settingsConfig: () =>
    api.get<SettingsConfigResponse>('/api/admin/settings/config', undefined, { useAdminToken: true }),
  /** PUT /config/:section — section ∈ 白名单;typed section 强类型校验,其余裸 JSON 透传 */
  putSettingsSection: (section: string, json: Record<string, unknown>) =>
    api.put<SettingsSection>(`/api/admin/settings/config/${section}`, json, { useAdminToken: true }),
  /** GET /snapshots — 倒序,上限 100 */
  settingsSnapshots: () =>
    api.get<SettingsSnapshotListResponse>('/api/admin/settings/snapshots', undefined, { useAdminToken: true }),
  /** POST /snapshots — 创建快照(全 section 聚合) */
  createSettingsSnapshot: (label?: string) =>
    api.post<SettingsSnapshotMeta>('/api/admin/settings/snapshots', label !== undefined ? { label } : {}, { useAdminToken: true }),
  /** POST /snapshots/:id/restore — 回滚;不存在 404 */
  restoreSettingsSnapshot: (id: number) =>
    api.post<SettingsConfigResponse>(`/api/admin/settings/snapshots/${id}/restore`, undefined, { useAdminToken: true }),
  /** GET /export.toml — TOML 文本(text/plain);keys/secrets 永不导出。走原始 fetch 避开 JSON unwrap。 */
  exportSettingsToml: async (): Promise<string> => {
    const token = tokenManager.getAdminToken();
    const res = await fetch(buildUrl('/api/admin/settings/export.toml'), {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!res.ok) throw new Error(`导出失败 HTTP ${res.status}`);
    return res.text();
  },

  // ═══════════ be:settings-rbac-keys:RBAC 管理员 ═══════════
  rbacAdmins: () =>
    api.get<RbacAdminListResponse>('/api/admin/settings/admins', undefined, { useAdminToken: true }),
  /** email 冲突 409 */
  createRbacAdmin: (payload: { email: string; password: string; role?: AdminRole }) =>
    api.post<RbacAdmin>('/api/admin/settings/admins', payload, { useAdminToken: true }),
  /** 降级最后一个 super-admin 409 LAST_SUPER_ADMIN;不存在 404 */
  updateRbacAdminRole: (id: string, role: AdminRole) =>
    api.patch<RbacAdmin>(`/api/admin/settings/admins/${id}/role`, { role }, { useAdminToken: true }),
  /** 删自己 400 CANNOT_DELETE_SELF;删最后 super-admin 409;不存在 404 */
  deleteRbacAdmin: (id: string) =>
    api.delete<RbacAdminDeleteResponse>(`/api/admin/settings/admins/${id}`, { useAdminToken: true }),

  // ═══════════ be:settings-rbac-keys:API Keys ═══════════
  apiKeys: () =>
    api.get<ApiKeyListResponse>('/api/admin/settings/api-keys', undefined, { useAdminToken: true }),
  /** 明文仅此一次 */
  createApiKey: (payload: { name: string; scope?: ApiKeyScope; expiresAt?: string }) =>
    api.post<ApiKeyCreateResponse>('/api/admin/settings/api-keys', payload, { useAdminToken: true }),
  /** 新明文一次,旧失效;不存在 404 */
  rotateApiKey: (id: string) =>
    api.post<ApiKeyCreateResponse>(`/api/admin/settings/api-keys/${id}/rotate`, undefined, { useAdminToken: true }),
  /** 不存在 404 */
  deleteApiKey: (id: string) =>
    api.delete<ApiKeyRevokeResponse>(`/api/admin/settings/api-keys/${id}`, { useAdminToken: true }),

  // ═══════════ be:amas-advisor-sandbox:沙箱试运行 ═══════════
  amasSandboxSuggestion: (id: number) =>
    api.post<SandboxSuggestionResponse>(`/api/admin/amas/advisor/suggestions/${id}/sandbox`, undefined, { useAdminToken: true }),

  // ═══════════ be:amas-config-canary:人群过滤 canary + diff-impact ═══════════
  /** PUT /amas/config/canary 扩展版(带 crowd-filter,响应含真实 audience)。
   *  与既有 amasSetCanary 同端点,此重载透出 audience + crowdFilters。 */
  amasSetCanaryExt: (payload: SetCanaryExtRequest) =>
    api.put<SetCanaryExtResponse>('/api/admin/amas/config/canary', payload, { useAdminToken: true }),
  /** POST /amas/config/diff-impact — patch 对比当前 live config 的指标影响估计 */
  amasConfigDiffImpact: (patch: Record<string, unknown>) =>
    api.post<DiffImpactResponse>('/api/admin/amas/config/diff-impact', { patch }, { useAdminToken: true }),

  // ═══════════ be:feedback-announce:公告 / FAQ ═══════════
  feedbackAnnouncements: (params?: { kind?: AnnouncementKind; published?: boolean }) =>
    api.get<{ data: FeedbackAnnouncement[] }>(
      '/api/admin/feedback/announcements',
      params as Record<string, string | number | boolean | undefined>,
      { useAdminToken: true },
    ),
  createFeedbackAnnouncement: (payload: { title: string; body: string; kind?: AnnouncementKind; published?: boolean }) =>
    api.post<FeedbackAnnouncement>('/api/admin/feedback/announcements', payload, { useAdminToken: true }),
  updateFeedbackAnnouncement: (id: string, payload: { title?: string; body?: string; kind?: AnnouncementKind; published?: boolean }) =>
    api.patch<FeedbackAnnouncement>(`/api/admin/feedback/announcements/${id}`, payload, { useAdminToken: true }),
  deleteFeedbackAnnouncement: (id: string) =>
    api.delete<{ deleted: boolean }>(`/api/admin/feedback/announcements/${id}`, { useAdminToken: true }),

  // ═══════════ be:feedback-announce:工单回复草稿(每工单一份 upsert) ═══════════
  getFeedbackDraft: (feedbackId: string) =>
    api.get<{ draft: FeedbackReplyDraft | null }>(`/api/admin/feedback/${feedbackId}/draft`, undefined, { useAdminToken: true }),
  saveFeedbackDraft: (feedbackId: string, payload: { body: string; pushInapp?: boolean }) =>
    api.post<FeedbackReplyDraft>(`/api/admin/feedback/${feedbackId}/draft`, payload, { useAdminToken: true }),
};

// ─────────── m022:新端点附属类型 ───────────
export interface AmasCanaryConfig {
  id: number;
  versionHash: string;
  percent: number;
  forceUserIds: string[];
  createdAt: string;
  createdBy: string;
}

export interface WordbookRankRow {
  wordbookId: string;
  name: string;
  learnerCount: number;
  recordCount: number;
  correctCount: number;
  accuracy: number | null;
}

export interface UserProfile {
  userId: string;
  totalRecords: number;
  correctRecords: number;
  accuracy: number | null;
  /** m026:平均反应时(ms);null = 无答题或全为 0 */
  avgResponseTimeMs: number | null;
  sessionCount: number;
  wordbookDistribution: Array<{ wordbookId: string; name: string; recordCount: number }>;
}

export interface UserSessionRow {
  id: string;
  status: string;
  createdAt: string;
  completedAt: string | null;
  wordsCount: number | null;
  correctCount: number | null;
  accuracy: number | null;
}

export interface BulkUserResult {
  total: number;
  succeeded: number;
  failed: number;
  results: Array<{ userId: string; success: boolean; error: string | null }>;
}

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

/** PR-1:metrics tab 顶部 4 KPI（决策总数/命中率/平均决策耗时/疲劳触发率） */
export interface AmasMetricsKpi {
  decisionTotal: number;
  hitCount: number;
  hitRate: number;
  avgLatencyMs: number;
  fatigueTriggerCount: number;
  fatigueTriggerRate: number;
  fatigueThreshold: number;
}

/** PR-1:8 算法决策分布甜甜圈单项（algorithm 为小写算法名，pct 为 0-1 占比） */
export interface AmasAlgorithmShare {
  algorithm: string;
  count: number;
  pct: number;
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
  /** m022:llm_advisor 写入时按 patch 的 key 从 basedOn 配置抓出的旧值快照,供 UI diff */
  baseValuesJson?: Record<string, number> | null;
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

// ─────────── 看板对齐设计稿新增类型（camelCase 镜像后端序列化） ───────────

/** 阶段分布：cold/transition/stable + 7d 趋势 */
export interface AmasStageStat {
  stage: 'cold' | 'transition' | 'stable';
  users: number;
  pct: number;
  avgDecisions: number;
  retention7d: number;
  mainRoute: string;
}
export interface AmasStageTrendPoint {
  date: string;
  cold: number;
  transition: number;
  stable: number;
}
export interface AmasStageDistribution {
  totalUsers: number;
  stages: AmasStageStat[];
  trend: AmasStageTrendPoint[];
}

/** ELO 散点：x=elo, y=decisions, color=deltaElo(7d) */
export interface AmasEloPoint {
  elo: number;
  decisions: number;
  deltaElo: number;
}
export interface AmasEloScatter {
  points: AmasEloPoint[];
  total: number;
  meanElo: number;
}

/** MDM 遗忘热图：days × 14 难度段，cell=平均遗忘概率（-1 表示空白） */
export interface AmasMdmHeatmap {
  days: string[];
  bandCount: number;
  cells: number[][];
  peak: number;
}

/** 疲劳信号时序 */
export interface AmasFatiguePoint {
  date: string;
  avgFatigue: number;
  peakFatigue: number;
  triggerCount: number;
}
export interface AmasFatigueTimeseries {
  points: AmasFatiguePoint[];
  avgIntensity: number;
  totalTriggers: number;
  threshold: number;
}

/** 每用户决策数直方图 */
export interface AmasDecisionBucket {
  label: string;
  count: number;
}
export interface AmasDecisionHistogram {
  buckets: AmasDecisionBucket[];
  p50: number;
  p95: number;
  totalUsers: number;
}

/** 状态流转（窗口内阶段穿越） */
export interface AmasTransition {
  from: string;
  to: string;
  count: number;
}
export interface AmasStateTransitions {
  windowHours: number;
  transitions: AmasTransition[];
}

/** 学习风格聚类（k-means k=4） */
export interface AmasLearningCluster {
  label: string;
  count: number;
  pct: number;
  avgResponseMs: number;
  errorRate: number;
  recordsPerActiveDay: number;
}
export interface AmasLearningStyleClusters {
  k: number;
  totalUsers: number;
  clusters: AmasLearningCluster[];
}

/** 异常分级 + 逐条列表 */
export interface AmasAnomalyItem {
  id: string;
  timestamp: string;
  severity: 'error' | 'warn' | 'info';
  code: string;
  title: string;
  userId: string;
  value: number;
  expectedRange: string;
  impactedUsers: number;
  impactPct: number;
}
export interface AmasAnomalySeveritySummary {
  error: number;
  warn: number;
  info: number;
  invariantPass: number;
  invariantTotal: number;
  passRate: number;
}
export interface AmasAnomalyFeed {
  summary: AmasAnomalySeveritySummary;
  items: AmasAnomalyItem[];
}

/** 版本对比扩展切片 */
export interface AmasVersionSliceExt {
  versionHash: string;
  eventCount: number;
  hitRate: number;
  p95LatencyMs: number;
  meanLatencyMs: number;
  fatigueRate: number;
  ensembleShare: number;
  meanReward: number;
  anomalyRate: number;
  retention7d: number;
  p95CompletionMs: number;
  spark: number[];
  configEpsilon: number | null;
  firstEventAt: string | null;
  lastEventAt: string | null;
}

// ─────────── advisor 全栈对齐类型（camelCase 镜像后端序列化） ───────────
export interface AdvisorCostStats {
  monthYuan: number;
  monthCapYuan: number;
  quotaPct: number;
  forecastYuan: number;
  avg7dCostYuan: number;
  monthCalls: number;
  acceptedCount: number;
  rejectedCount: number;
  acceptanceRate: number;
  /** USD→CNY 汇率(后端可配置),前端单条建议成本¥换算用,勿硬编码 */
  usdToCny: number;
}

export interface AdvisorCostDaily {
  date: string;
  costYuan: number;
}

export interface AdvisorConfig {
  model: string;
  pollCron: string;
  apiKeyTail: string;
  monthCapYuan: number;
  autoApplyEnabled: boolean;
  autoApplyMaxPerDay: number;
  autoApplyMinConfidence: number;
  grayscaleSteps: [number, number, number];
  advisorEnabled: boolean;
}

export interface WhitelistRow {
  path: string;
  minSafe: number;
  maxSafe: number;
}

export type PatchCanaryStatus = 'active' | 'effective' | 'rolled_back';

export interface PatchCanary {
  id: number;
  suggestionId: number;
  versionHash: string;
  percent: number;
  cohortLo: number;
  cohortHi: number;
  status: PatchCanaryStatus;
  baselineMetricsJson: string;
  startedAt: string;
  updatedAt: string;
}

/** 仅 GET /advisor/canary 列表端点联表附带实测口径;create/scale 端点返回基础 PatchCanary 不含这三字段。 */
export interface PatchCanaryWithMetrics extends PatchCanary {
  liveReward: number;
  liveAnomalyRate: number;
  baselineReward: number;
}

// ─────────── T1.3：真实留存 A/B 实验（camelCase 镜像后端序列化） ───────────
export type ExperimentStatus = 'running' | 'concluded_adopt' | 'concluded_reject';

export interface AmasExperiment {
  experimentId: string;
  suggestionId: number | null;
  canaryVersionHash: string;
  baselineVersionHash: string;
  canaryCohortLo: number;
  canaryCohortHi: number;
  primaryMetric: string;
  minSample: number;
  alpha: number;
  power: number;
  mde: number;
  offlineDelta: number | null;
  status: ExperimentStatus;
  registeredAt: string;
  concludedAt: string | null;
  notes: string | null;
}

export interface RegisterExperimentPayload {
  experimentId?: string;
  suggestionId?: number | null;
  canaryVersionHash: string;
  baselineVersionHash: string;
  canaryCohortLo: number;
  canaryCohortHi: number;
  primaryMetric?: string;
  minSample: number;
  alpha?: number;
  power?: number;
  mde: number;
  offlineDelta?: number | null;
  notes?: string | null;
}

export interface PlanExperimentPayload {
  kind?: 'proportion' | 'mean';
  alpha?: number;
  power?: number;
  dailySignups?: number;
  p0?: number;
  mdeRel?: number;
  sigma?: number;
  delta?: number;
}

export interface AmasProportionRaw { successes: number; n: number }
export interface AmasMeanRaw { mean: number; var: number; n: number }
export interface AmasArmRaw {
  arm: string;
  enrolled: number;
  day7Retention: AmasProportionRaw;
  day30Retention: AmasProportionRaw;
  reviewsPerDay: AmasMeanRaw;
  sessionCompletion: AmasProportionRaw;
  masteredHold: AmasProportionRaw;
}
export interface AmasExperimentRawMetrics {
  experimentId: string;
  canary: AmasArmRaw;
  baseline: AmasArmRaw;
}
export interface AmasMetricPoint { value: number; ciLow: number; ciHigh: number; n: number }
export interface AmasMetricComparison {
  metric: string;
  kind: 'proportion' | 'mean';
  higherIsBetter: boolean;
  canary: AmasMetricPoint;
  baseline: AmasMetricPoint;
  diff: number;
  pValue: number | null;
  significant: boolean;
  regressed: boolean;
}
export interface AmasExperimentVerdict {
  experimentId: string;
  primaryMetric: string;
  alpha: number;
  mde: number;
  minSample: number;
  metrics: AmasMetricComparison[];
  sampleOk: boolean;
  primarySignificant: boolean;
  primaryPositive: boolean;
  primaryMeetsMde: boolean;
  guardrailRegressions: string[];
  adoptRecommended: boolean;
  reason: string;
}
export interface AmasExperimentMetricsResponse {
  experiment: AmasExperiment;
  raw: AmasExperimentRawMetrics;
  verdict: AmasExperimentVerdict;
}
export interface AmasExperimentPlan {
  minSamplePerArm: number;
  totalRequired: number;
  estimatedDays: number | null;
  recommendedPercent: number;
}

// ─────────── Analytics 看板深化响应类型（camelCase 镜像后端序列化） ───────────

/** KPI 卡：同比上一等长窗口。retention 用百分点(deltaPt)，其余用增长率(deltaPct)。 */
export interface AnalyticsKpiPctDelta {
  value: number;
  prevValue: number;
  /** (cur-prev)/prev；prev=0 时为 null */
  deltaPct: number | null;
}
export interface AnalyticsKpiPtDelta {
  /** 0-1 小数 */
  value: number;
  prevValue: number;
  /** cur-prev（百分点） */
  deltaPt: number | null;
}
export interface AnalyticsKpiSummary {
  generatedAt: string;
  days: number;
  rangeStart: string;
  rangeEnd: string;
  /** 本窗口新注册数 */
  newRegistrations: AnalyticsKpiPctDelta;
  /** 窗口内日活均值（每日 distinct 答题用户取平均，四舍五入） */
  dauAverage: AnalyticsKpiPctDelta;
  /** d7 留存 0-1 小数 */
  d7Retention: AnalyticsKpiPtDelta;
  /** 窗口内 learning_sessions.summary_duration_secs 求和 */
  studyDurationSecs: AnalyticsKpiPctDelta;
}

/** 漏斗单步骤 */
export interface AnalyticsFunnelStep {
  key: string;
  label: string;
  sublabel: string;
  count: number;
  /** 0-1，相对 register */
  pct: number;
  /** vs 上一等长窗口同步骤 pct（百分点）；无可比时 null */
  deltaPt: number | null;
  tone: 'good' | 'normal' | 'warn';
}
export interface AnalyticsFunnel {
  generatedAt: string;
  days: number;
  rangeStart: string;
  rangeEnd: string;
  /** 仅统计"在本窗口注册"的用户队列 */
  steps: AnalyticsFunnelStep[];
  biggestDropFrom: string;
  biggestDropTo: string;
  biggestDropPt: number;
}

/** cohort 留存矩阵：按注册周分组 */
export interface AnalyticsRetentionCohort {
  /** 周一，YYYY-MM-DD */
  cohortStart: string;
  size: number;
  /** 长度=weeks；cells[k]=注册后第 k 周仍活跃用户/size，0-1；cells[0]=1.0；未过完的周=null */
  cells: (number | null)[];
}
export interface AnalyticsRetentionMatrix {
  generatedAt: string;
  weeks: number;
  /** 按 cohortStart 升序，最多最近 weeks 个 cohort */
  cohorts: AnalyticsRetentionCohort[];
}

/** 题型分布 + 难度分箱 */
export interface AnalyticsQuestionTypeStat {
  key: string;
  label: string;
  count: number;
  /** count/totalRecords，0-1 */
  pct: number;
}
export interface AnalyticsDifficultyBin {
  label: string;
  min: number | null;
  max: number | null;
  count: number;
  /** 0-1 */
  pct: number;
}
export interface AnalyticsQuestionDistribution {
  generatedAt: string;
  days: number;
  totalRecords: number;
  questionTypes: AnalyticsQuestionTypeStat[];
  /** 按 word_elo.rating 分 5 箱（无评分按 1200 计） */
  difficultyBins: AnalyticsDifficultyBin[];
}

/** 高频词 */
export interface AnalyticsWordFrequencyRow {
  rank: number;
  wordId: string;
  spelling: string;
  pos: string | null;
  recordCount: number;
  /** AVG(is_correct)，0-1；无答题为 null */
  accuracy: number | null;
  elo: number | null;
  /** be:analytics-misc:窗口内答该词用户的 mastery_level 均值(0..1),无数据 null */
  mastery: number | null;
}
export interface AnalyticsWordFrequency {
  generatedAt: string;
  days: number;
  limit: number;
  sort: string;
  rows: AnalyticsWordFrequencyRow[];
}

/** 自动洞察条目（后端启发式产出 3-4 条） */
export interface AnalyticsInsightItem {
  tone: 'success' | 'warning' | 'info' | 'accent';
  title: string;
  body: string;
}
export interface AnalyticsInsights {
  generatedAt: string;
  days: number;
  items: AnalyticsInsightItem[];
}

// ═══════════════════════════════════════════════════════════════════
//  新一批后端端点附属类型（settings / rbac / api-keys / setup /
//  health / facets / amas sandbox+diff-impact / feedback 公告·草稿）
//  —— 与 adminApi 同文件 colocate，供 per-page agents 直接 import。
// ═══════════════════════════════════════════════════════════════════

/* ---------- be:analytics-misc:users.html 筛选 chip 计数 ---------- */
export interface UserFacets {
  total: number;
  active: number;
  /** 7 天未登录(含从未登录) */
  inactive7d: number;
  banned: number;
  admins: number;
}

/* ---------- be:login-health:/health 公开端点 ---------- */
export interface HealthInfo {
  status?: string;
  /** CARGO_PKG_VERSION，对应 footer "vX.Y.Z" */
  version: string;
  /** sqlite 主库+wal+shm 磁盘占用之和；内存库/缺失/失败为 null(前端展示 "—") */
  dbSizeBytes: number | null;
  /** 可用性：源自 http_metrics 真实 5xx 错误率聚合；无请求样本时为 null(展示 "—")。
   *  effectiveSecs = 实际测量窗口(≤30d，进程重启清零)，前端据此动态标注真实窗口而非冒充 30d。 */
  availability?: {
    pct: number;
    effectiveSecs: number;
    totalRequests: number;
  } | null;
  [key: string]: unknown;
}

/* ---------- be:setup-envcheck:登录前环境自检 ---------- */
export interface EnvCheckItem {
  key: string;
  label: string;
  ok: boolean;
  detail: string;
}
export interface EnvCheckResponse {
  /** 固定 5 项顺序:db_schema/static_writable/listening_port/signing_key/admin_table */
  checks: EnvCheckItem[];
}

/* ---------- be:settings-* :通用配置存储 ---------- */
/** 11 个设置面板的 section 标识(含 typed 与裸 JSON 透传两类) */
export type SettingsSectionKey =
  | 'site' | 'auth' | 'ratelimit' | 'audit-config' | 'backup-policy'
  | 'roles' | 'email' | 'smtp' | 'sms' | 'sms_push'
  | 'storage' | 'db' | 'db_cache' | 'limits' | 'audit' | 'backup' | 'keys';

export interface SettingsSection {
  section: string;
  /** 归一化后的 section JSON(密钥字段在 GET/PUT 响应里已遮蔽为 ••••••) */
  json: Record<string, unknown>;
  updatedAt: string;
}
export interface SettingsConfigResponse {
  sections: SettingsSection[];
}
export interface SettingsSnapshotMeta {
  id: number;
  label: string | null;
  createdAt: string;
}
export interface SettingsSnapshotListResponse {
  snapshots: SettingsSnapshotMeta[];
}

/* ---------- be:settings-rbac-keys:RBAC 管理员 ---------- */
export type AdminRole = 'super_admin' | 'admin';
export interface RbacAdmin {
  id: string;
  email: string;
  role: AdminRole;
  createdAt: string;
  lockedUntil: string | null;
}
export interface RbacAdminListResponse {
  admins: RbacAdmin[];
}
export interface RbacAdminDeleteResponse {
  deleted: boolean;
  adminId: string;
}

/* ---------- be:settings-rbac-keys:API Keys ---------- */
export type ApiKeyScope = 'read' | 'write' | 'admin';
/** 掩码视图,绝不含明文/hash */
export interface ApiKey {
  id: string;
  name: string;
  scope: ApiKeyScope;
  prefix: string;
  createdAt: string;
  createdBy: string | null;
  expiresAt: string | null;
  lastUsedAt: string | null;
  revokedAt: string | null;
}
export interface ApiKeyListResponse {
  keys: ApiKey[];
}
/** create / rotate 返回:明文仅此一次 */
export interface ApiKeyCreateResponse {
  key: ApiKey;
  plaintext: string;
  message: string;
}
export interface ApiKeyRevokeResponse {
  revoked: boolean;
  keyId: number;
}

/* ---------- be:amas-advisor-sandbox:沙箱试运行 ---------- */
export type SandboxConfidence = 'high' | 'medium' | 'low';
export interface SandboxChange {
  path: string;
  from: unknown;
  to: unknown;
  relChange: number | null;
  inWhitelist: boolean;
}
export interface SandboxMetricImpact {
  baseline: number | null;
  predicted: number | null;
  deltaPt: number;
}
export interface SandboxSuggestionResponse {
  suggestionId: number;
  basedOnVersionHash: string;
  configValid: boolean;
  configError: string | null;
  whitelistOk: boolean;
  whitelistErrors: string[];
  changes: SandboxChange[];
  accuracy: SandboxMetricImpact;
  fatigue: SandboxMetricImpact;
  /** d7 baseline 恒 null(telemetry 无留存维度),仅返回预估 delta */
  d7Retention: SandboxMetricImpact;
  confidence: SandboxConfidence;
  method: string;
  telemetrySampleSize: number;
}

/* ---------- be:amas-config-canary:人群过滤 + diff-impact ---------- */
export interface CanaryCrowdFilters {
  minAccountAgeDays?: number;
  preferActive?: boolean;
  webOnly?: boolean;
  autoScale24h?: boolean;
}
export interface CanaryAudience {
  totalUsers: number;
  eligibleUsers: number;
  affectedUsers: number;
  /** 当前恒为 "estimate-only"(引擎未按 per-user 人群分流) */
  enforcement: string;
}
/** PUT /amas/config/canary 扩展请求(在 versionHash/percent/forceUserIds 上新增过滤) */
export interface SetCanaryExtRequest {
  versionHash?: string;
  percent?: number;
  forceUserIds?: string[];
  minAccountAgeDays?: number;
  preferActive?: boolean;
  webOnly?: boolean;
  autoScale24h?: boolean;
}
export interface SetCanaryExtResponse {
  canary: AmasCanaryConfig & { crowdFilters?: CanaryCrowdFilters };
  audience: CanaryAudience;
}
export type DiffImpactMetric = 'accuracy' | 'fatigue' | 'd7Retention';
export interface DiffImpactEntry {
  metric: DiffImpactMetric;
  deltaLowPt: number;
  deltaHighPt: number;
  direction: string;
}
export interface DiffImpactField {
  path: string;
  from: unknown;
  to: unknown;
  relChange: number | null;
  inWhitelist: boolean;
  impacts: DiffImpactEntry[];
  confidence: SandboxConfidence;
}
export interface DiffImpactResponse {
  fields: DiffImpactField[];
  telemetrySampleSize: number;
  confidence: SandboxConfidence;
  method: string;
}

/* ---------- be:feedback-announce:公告 / FAQ + 回复草稿 ---------- */
export type AnnouncementKind = 'announcement' | 'faq';
export interface FeedbackAnnouncement {
  id: string;
  title: string;
  body: string;
  kind: AnnouncementKind;
  published: boolean;
  authorId: string | null;
  createdAt: string;
  updatedAt: string;
}
export interface FeedbackReplyDraft {
  feedbackId: string;
  body: string;
  pushInapp: boolean;
  authorId: string | null;
  updatedAt: string;
}

/* ---------- be:broadcast-packs-minor:广播分页元信息 ---------- */
export interface BroadcastPagination {
  total: number;
  offset: number;
  limit: number;
}

/* ---------- m042/D2:推送编辑器草稿 ---------- */
export interface PushDraft {
  title: string;
  message: string;
  platforms: string[];
  versionMin: string | null;
  lastActiveDays: number | null;
  authorId: string | null;
  updatedAt: string;
}
