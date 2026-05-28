import type { PaginatedResponse } from './api';

/** m024:批量答题聚合;list?includeStats=true 时填充。 */
export interface AdminUserStats {
  recordCount: number;
  correctCount: number;
  /** 最近 20 题 is_correct(0/1)序列,从新到旧;不足 20 时 len<20 */
  last20Outcomes: number[];
}

export interface AdminUser {
  id: string;
  email: string;
  username: string;
  isBanned: boolean;
  failedLoginCount: number;
  lockedUntil: string | null;
  createdAt: string;
  updatedAt: string;
  /** m024:'user' / 'staff' / 'admin' */
  role: 'user' | 'staff' | 'admin';
  /** m024:'active' / 'inactive' / 'suspended' */
  status: 'active' | 'inactive' | 'suspended';
  /** m024:最近一次登录时间;null 表示从未登录 */
  lastLoginAt: string | null;
  /** m025:注册来源(referral/marketing channel);null 表示未知 */
  referrerSource: string | null;
  /** m024:仅 list?includeStats=true 时填充 */
  stats?: AdminUserStats;
}

export interface AdminUsersQuery {
  page?: number;
  perPage?: number;
  search?: string;
  banned?: boolean;
  /** m024:角色筛选 */
  role?: 'user' | 'staff' | 'admin';
  /** m024:状态筛选 */
  status?: 'active' | 'inactive' | 'suspended';
  /** m024:最近 N 天未登录(含从未登录),0 表示不过滤 */
  inactiveDays?: number;
  /** m024:为 true 时 list 返回每个 user 的答题聚合 */
  includeStats?: boolean;
}

export type AdminUsersPage = PaginatedResponse<AdminUser>;

/** m024:创建用户 payload */
export interface AdminCreateUserPayload {
  email: string;
  username: string;
  password: string;
  role?: 'user' | 'staff' | 'admin';
}

/** m024:某用户绑定的 client_device 行,供 Drawer "设备 / 会话" tab */
export interface ClientDeviceRow {
  deviceId: string;
  platform: string;
  userId: string | null;
  firstSeenAt: string;
  lastSeenAt: string;
  isBanned: boolean;
  bannedAt: string | null;
  bannedBy: string | null;
  banReason: string | null;
  appVersion: string | null;
  /** m027:GeoIP 反查 ISO-3166-1 alpha-2;无 mmdb / 私网 IP 时为 null */
  country?: string | null;
  /** m027:仅 Drawer 详情用,列表不展示 */
  lastIp?: string | null;
}

/** m027:设备表后端分页行(精简字段集,不含 banned_by/ban_reason 等审计敏感列) */
export interface ListedDevice {
  deviceId: string;
  platform: string;
  userId: string | null;
  appVersion: string | null;
  country: string | null;
  firstSeenAt: string;
  lastSeenAt: string;
  isBanned: boolean;
}

/** m027:平台聚合 hero 卡片数据源 */
export interface ClientPlatformAgg {
  platform: string;
  total: number;
  active7d: number;
  /** 月环比百分比(可负;原表为空时为 0.0 或 100.0) */
  monthOverMonthPct: number;
}

/** m027:平台×版本分布柱(按 platform 分组,版本按 count 倒序) */
export interface ClientVersionAgg {
  platform: string;
  /** 'unknown' 表示 app_version IS NULL */
  version: string;
  count: number;
}

/** m027:强制升级策略一行(每平台独立) */
export interface ClientUpgradePolicy {
  platform: string;
  minVersion: string | null;
  suggestedVersion: string | null;
  grayscalePct: number;
  pwaSilentUpdate: boolean;
  updatedAt: string;
  updatedBy: string | null;
}

/** m024:admin 操作审计行(target_type='user'),供 Drawer "操作日志" tab */
export interface AdminAuditEntry {
  id: string;
  adminId: string;
  fromVersion: string;
  toVersion: string;
  channel: string;
  startedAt: string;
  completedAt: string | null;
  outcome: string;
  error: string | null;
  action: string;
  targetType: string | null;
  targetId: string | null;
  metadataJson: string | null;
}

/** m025:用户**自有**活动日志(login / session.complete / goal.update / fatigue.alert) */
export interface UserActivityEntry {
  id: string;
  userId: string;
  action: string;
  detailJson: string | null;
  ip: string | null;
  createdAt: string;
}

/** m025:用户档案深化数据;字段全为可选(数据未初始化时缺) */
export interface UserExtras {
  preferences: {
    theme: string;
    language: string;
    notificationEnabled: boolean;
    soundEnabled: boolean;
  } | null;
  elo: { rating: number; sigma: number; games: number; level: number } | null;
  habit: {
    dailyGoalWords: number;
    dailyGoalMinutes: number;
    sessionsPerDay: number;
    medianSessionMins: number;
    totalSessions: number;
  } | null;
  streakDays: number;
  /** m026:用户选中的词书(JOIN study_configs.selected_wordbook_ids × wordbooks)*/
  selectedWordbooks: Array<{ id: string; name: string }>;
  latestStrategy: { strategy: Record<string, unknown>; at: string } | null;
  latestDevice: {
    osName: string | null;
    browserName: string | null;
    browserVersion: string | null;
    timezone: string | null;
    language: string | null;
  } | null;
  metrics7d: {
    records: number;
    correct: number;
    accuracy: number | null;
    fatigueAlertCount: number;
  };
}

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
  /** 生命周期内 5xx 错误率（0.0–1.0），M0-P1 计数器实装 */
  errorRate: number;
  /** m023:Dashboard 系统资源条数据源。任一字段为 null 时前端不渲染对应进度条。 */
  resources?: SystemResources;
}

export interface SystemResources {
  /** 进程 CPU 占用百分比(0–100)。多核累加,可能 >100。null 表示采样失败。 */
  cpuPct: number | null;
  /** 进程 RSS 字节数 */
  memoryRssBytes: number | null;
  /** cwd 所在磁盘总容量字节 */
  diskTotalBytes: number | null;
  /** cwd 所在磁盘剩余可用字节 */
  diskFreeBytes: number | null;
  /** r2d2 连接池占用快照 */
  pool: { max: number; connections: number; idle: number } | null;
}

export interface WorkerStatusRow {
  workerName: string;
  lastRunAt: string | null;
  lastDurationMs: number | null;
  lastOutcome: string | null;
  lastError: string | null;
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

/// 单通道视图：每个 channel 的最新 release 元数据 + 升级判定。
/// v0.6.0-beta.3 起 Stable 与 Beta 通道并存，共用此结构。
export interface ChannelStatus {
  latestVersion: string;
  latestPublishedAt: string | null;
  releaseNotes: string;
  releaseUrl: string;
  hasUpdate: boolean;
  /// 当前进程能否用这条 release 自更新：架构匹配 + 找到 tar.gz / sha256 资产对。
  canApply: boolean;
}

/// `/api/admin/updates/status` 与 `/check` 共用。
/// v0.6.0-beta.3：stable + beta 双通道嵌套替换原扁平 latestVersion 等字段。
/// v0.5.2 起 status 端点同时返回 `applyTask` 后台任务状态。
export interface AdminUpdateStatus {
  currentVersion: string;
  stable: ChannelStatus | null;
  beta: ChannelStatus | null;
  lastCheckedAt: string | null;
  autoCheckEnabled: boolean;
  allowDowngrade: boolean;
  /// v0.5.2+ 后端 spawn 的异步升级任务进度；首次发起前为 undefined
  applyTask?: ApplyTaskStatus;
}

/// admin 一键升级后台任务状态（v0.5.2+，配合异步 apply）。
/// `phase` 取值：`pending` | `downloading` | `verifying` | `extracting`
///                | `backing_up_db` | `swapping` | `restarting` | `completed` | `failed`
export interface ApplyTaskStatus {
  taskId: string;
  phase: string;
  percent: number;
  targetVersion: string;
  startedAt: string;
  completedAt?: string;
  error?: string;
}

/// S5：升级历史审计记录。outcome: `success` | `failed` | `in_progress`
export interface UpdateAuditEntry {
  id: string;
  adminId: string;
  fromVersion: string;
  toVersion: string;
  channel: string;
  startedAt: string;
  completedAt?: string;
  outcome: 'success' | 'failed' | 'in_progress';
  error?: string;
}

/// `POST /api/admin/updates/apply` 立即返回（202 Accepted）的载荷
export interface ApplyAccepted {
  taskId: string;
  phase: string;
  percent: number;
  targetVersion: string;
  startedAt: string;
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

/**
 * 用户反馈条目，对应后端 `FeedbackItem`（src/store/operations/feedback.rs）。
 * 后端 list_feedback 暴露在 /api/admin/feedback。
 * M1-G3：补全 priority / status / assigneeAdminId / resolvedAt / resolution 字段。
 */
export interface FeedbackItem {
  id: string;
  userId: string;
  category: string | null;
  body: string;
  route: string | null;
  createdAt: string;
  /** 优先级：'low' | 'normal' | 'high' | 'urgent'，后端默认 'normal' */
  priority: string;
  /** 处理状态：'open' | 'in_progress' | 'resolved' | 'closed'，后端默认 'open' */
  status: string;
  /** 处理人 admin ID，可为 null */
  assigneeAdminId: string | null;
  /** 解决时间，ISO 8601，可为 null */
  resolvedAt: string | null;
  /** 解决备注，可为 null */
  resolution: string | null;
  /** m022:用户提交时附带的设备指纹快照(平台/版本/OS 等任意 JSON),旧客户端不传时为 null */
  deviceProfile?: Record<string, unknown> | null;
  /** m022:答题上下文快照(最近 N 步事件 / 当前题目 ID 等任意 JSON),老客户端不传时为 null */
  answerSnapshot?: Record<string, unknown> | null;
}
