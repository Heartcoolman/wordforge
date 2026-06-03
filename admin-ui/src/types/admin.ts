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
  appVersion: string | null;
  /** m038:遥测硬识别上报的设备型号,未上报为 null */
  model: string | null;
  /** m027:GeoIP 反查 ISO-3166-1 alpha-2;无 mmdb / 私网 IP 时为 null */
  country: string | null;
  firstSeenAt: string;
  lastSeenAt: string;
  isBanned: boolean;
  // /users/:id/devices 走脱敏 DTO(ListedDevice),不含 lastIp/bannedBy/banReason
}

/** m027:设备表后端分页行(精简字段集,不含 banned_by/ban_reason 等审计敏感列) */
export interface ListedDevice {
  deviceId: string;
  platform: string;
  userId: string | null;
  appVersion: string | null;
  /** m038:遥测硬识别上报的设备型号,未上报为 null */
  model: string | null;
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
  /** DB ping 探针结果 */
  storeProbeOk?: boolean;
  /** m023:Dashboard 系统资源条数据源。任一字段为 null 时前端不渲染对应进度条。 */
  resources?: SystemResources;
  /** 子服务健康（监控页服务状态行 / 探针行数据源） */
  services?: {
    amas: { healthy: boolean };
    sse: { healthy: boolean; activeConnections: number; activeDevices: number; maxConnections?: number };
    wordbookCenter: { healthy: boolean; probeSkipped?: boolean };
  };
  /** S2-1:领域事件 outbox 异步消费健康（默认同步路径时恒 0）。 */
  outbox?: { pending: number; lagSecs: number; deadLetter: number };
}

/** W1-2:outbox 死信明细行。user 由后端 best-effort 从 payload 解析。 */
export interface DeadLetterEntry {
  id: number;
  eventType: string;
  userId: string | null;
  attempts: number;
  lastError: string;
  createdAt: string;
  deadAt: string;
}

export interface SystemResources {
  /** 进程 CPU 占用百分比(0–100)。多核累加,可能 >100。null 表示采样失败。 */
  cpuPct: number | null;
  /** 进程 RSS 字节数 */
  memoryRssBytes: number | null;
  /** m028:系统总内存字节(RSS 进度条分母);采样失败为 null */
  memoryTotalBytes?: number | null;
  /** cwd 所在磁盘总容量字节 */
  diskTotalBytes: number | null;
  /** cwd 所在磁盘剩余可用字节 */
  diskFreeBytes: number | null;
  /** m028:SQLite WAL 文件大小字节;0 = 无 WAL / 内存库 / stat 失败 */
  walSizeBytes?: number | null;
  /** r2d2 连接池占用快照 */
  pool: { max: number; connections: number; idle: number } | null;
  /** 当前在途(处理中)请求数。过载饱和度信号:CPU 钉死但 5xx=0 时,堆积即过载证据。 */
  inflightRequests?: number | null;
  /** nginx 边缘(stub_status)。未配置 NGINX_STATUS_URL 时为 null(功能关闭)。 */
  nginxEdge?: NginxEdge | null;
}

/** nginx stub_status 解析结果。accepts/handled/requests 为进程生命周期累计。 */
export interface NginxEdge {
  /** 当前活跃连接数 */
  active: number;
  /** 累计接受的连接 */
  accepts: number;
  /** 累计成功处理的连接(< accepts 即有连接被丢弃) */
  handled: number;
  /** 累计请求数 */
  requests: number;
  /** 累计被丢弃连接 = accepts - handled */
  dropped: number;
  /** 正在读请求头的连接 */
  reading?: number | null;
  /** 正在写响应(含等上游)的连接——高值=上游慢/饱和 */
  writing?: number | null;
  /** keepalive 空闲连接 */
  waiting?: number | null;
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
  /** m028:WAL 文件大小字节 */
  walSizeBytes?: number;
}

/** m028:滚动请求指标时序单点。t 为 unix 秒,前端格式化 HH:MM 标签。 */
export interface RequestMetricsPoint {
  t: number;
  qps: number;
  p99Ms: number;
}

/** m028:GET /api/admin/monitoring/requests —— SLO 卡 + 请求延迟图 + axum 服务行 rps 数据源。 */
export interface RequestMetrics {
  /** 请求窗口秒 */
  windowSecs: number;
  /** 实际可得窗口秒 = min(window, 自首条记录以来);可用性据此如实标注真实口径 */
  effectiveSecs: number;
  totalRequests: number;
  total5xx: number;
  qpsAvg: number;
  p50Ms: number;
  p99Ms: number;
  /** 窗口错误率 0.0–1.0 */
  errorRate: number;
  /** 窗口可用性百分比 (1-errorRate)*100 */
  availabilityPct: number;
  /** 入站带宽 字节/秒(Content-Length 口径) */
  bandwidthInBps: number;
  series: RequestMetricsPoint[];
}

/** m028:进程内日志环形缓冲单行。 */
export interface LogLine {
  /** unix 毫秒 */
  tsMs: number;
  /** ERROR / WARN / INFO / DEBUG / TRACE */
  level: string;
  target: string;
  message: string;
}

/** m028:派生告警时间线条目。resolved=周期任务成功完成(绿点)。 */
export interface AlertEvent {
  tsMs: number;
  severity: 'error' | 'warning' | 'info' | 'resolved';
  title: string;
  desc: string;
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
  /// release tarball 字节大小（0 表示未知 / 未找到资产）
  tarballSize: number;
  /// release tarball 的 sha256（check 时按需拉取，未知为 null）
  sha256: string | null;
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
  /// 当前二进制安装时间（current_exe mtime，rfc3339）；解析失败为 null
  installedAt: string | null;
  /// 进程运行时长（秒）
  uptimeSecs: number;
}

/// CHANGELOG 单条 commit（GitHub compare API 分类后）。
export interface ChangelogCommit {
  category: string;
  scope: string | null;
  subject: string;
  sha: string;
  author: string;
}

/// GET /api/admin/updates/changelog 响应。available=false 时回退渲染 releaseNotes。
export interface ChangelogSummary {
  available: boolean;
  channel?: string;
  targetVersion?: string;
  reason?: string;
  base?: string;
  head?: string;
  totalCommits?: number;
  contributors?: number;
  categoryCounts?: Record<string, number>;
  commits?: ChangelogCommit[];
  compareUrl?: string;
}

/// 备份种类：升级前 / 每日定时 / 手动 / 恢复前兜底。
export type BackupKind = 'upgrade' | 'daily' | 'manual' | 'pre_restore';

export interface BackupEntry {
  name: string;
  kind: BackupKind;
  sizeBytes: number;
  createdAt: string;
  version: string | null;
}

export interface BackupList {
  backups: BackupEntry[];
  totalBytes: number;
  thresholdBytes: number;
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

/// S5：升级历史审计记录。outcome: `success` | `failed` | `in_progress` | `rolled_back`
export interface UpdateAuditEntry {
  id: string;
  adminId: string;
  fromVersion: string;
  toVersion: string;
  channel: string;
  startedAt: string;
  completedAt?: string;
  outcome: 'success' | 'failed' | 'in_progress' | 'rolled_back';
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

/** 系统广播近 30 天聚合统计（GET /api/admin/broadcast） */
export interface BroadcastStats {
  total: number;
  totalSent: number;
  /** 平均阅读率 0..1 */
  avgReadRate: number;
  online: number;
}

/** W2-4:离站备份单 target 运行时状态（只读，与 backup-policy 配置解耦）。 */
export interface BackupTargetStatus {
  name: string;
  uri: string;
  /** 上次成功上传时间（RFC3339）；从未成功为 null。 */
  lastOkAt: string | null;
  /** 上次成功上传字节数。 */
  lastBytes: number | null;
  /** 上次失败原因；上次成功为 null。 */
  lastError: string | null;
  /** 上次尝试时间（成功或失败均更新）。 */
  lastAttemptAt: string;
}

/** W2-2:一条待发定时广播（队列视图，含受众回显）。 */
export interface ScheduledBroadcast {
  id: string;
  title: string;
  message: string;
  platforms: string[];
  versionMin: string | null;
  lastActiveDays: number | null;
  userIds: string[];
  /** 计划下发时间（RFC3339） */
  scheduledAt: string;
  createdAt: string;
}

/** 单条广播历史（近 30 天，按 createdAt 倒序） */
export interface BroadcastHistoryEntry {
  id: string;
  title: string;
  message: string;
  author: string;
  sentCount: number;
  readCount: number;
  /** 阅读率 0..1 */
  readRate: number;
  createdAt: string;
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
  canaryRewardDropThreshold: number;
  canaryAnomalyRiseThreshold: number;
  /** D4:客户端最低版本门控阈值(semver),null=未设置(回落 env) */
  minClientVersion?: string | null;
  /** D4:版本门控开关 */
  versionGateEnabled: boolean;
}

/** D4:客户端版本门控运行时配置(读/写端点共用) */
export interface VersionGate {
  enabled: boolean;
  minClientVersion?: string | null;
  /** env MIN_CLIENT_VERSION 兜底值(只读,供 UI 提示) */
  envMinClientVersion?: string | null;
  /** 实际生效阈值(settings 优先回落 env) */
  effectiveMinClientVersion?: string | null;
  /** strict-mode 全量契约校验是否启用(只读) */
  strictModeEnabled: boolean;
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
    /** be:analytics-misc:上一等长窗口答题数 */
    prevRecordCount: number;
    /** be:analytics-misc:上一窗口正确率,prev=0 时 null */
    prevAccuracy: number | null;
    /** be:analytics-misc:答题数环比 (cur-prev)/prev,prev=0 为 null */
    recordCountDeltaPct: number | null;
    /** be:analytics-misc:正确率环比百分点 cur-prev,任一窗口无答题为 null */
    accuracyDeltaPt: number | null;
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
  /** m030:首次被管理员阅读时间,null 表示未读 */
  readAt: string | null;
  /** m030:首次响应(回复或分派)时间,用于响应时长统计 */
  firstResponseAt: string | null;
  /** m030:用户提交的 CSAT 满意度评分(1-5),null 表示未评分 */
  csatScore: number | null;
  /** m030:CSAT 评分附带的文字评论 */
  csatComment: string | null;
  /** m030:被合并到此工单的重复反馈数量 */
  dedupCount: number;
  /** m030:关联的 GitHub Issue URL,null 表示未转 */
  githubIssueUrl: string | null;
  /** enrich-only:LEFT JOIN users 补充的用户名 */
  userName?: string | null;
  /** enrich-only:从 deviceProfile 解析的平台(web/ios/android) */
  platform?: string | null;
  /** enrich-only:从 deviceProfile 解析的应用版本 */
  appVersion?: string | null;
}

/** m030:工单回复,对应后端 `FeedbackReply`。 */
export interface FeedbackReply {
  id: string;
  feedbackId: string;
  /** 'admin' | 'user' */
  authorKind: string;
  authorId: string | null;
  body: string;
  /** 是否推送应用内通知给用户 */
  pushInapp: boolean;
  /** 是否抄送邮箱 */
  ccEmail: boolean;
  createdAt: string;
}

/** m030:工单时间线事件,对应后端 `FeedbackEvent`。 */
export interface FeedbackEvent {
  id: string;
  feedbackId: string;
  /** 'submitted' | 'reply' | 'assigned' | 'resolved' | 'merged' | 'github' */
  kind: string;
  actor: string | null;
  summary: string;
  /** 关联实体 ID(如 reply id / target feedback id) */
  refId: string | null;
  createdAt: string;
}

/** m030:工单附件,对应后端 `FeedbackAttachment`。 */
export interface FeedbackAttachment {
  id: string;
  feedbackId: string;
  name: string;
  url: string;
  /** 'image' 等 */
  kind: string;
  createdAt: string;
}

/** m030:反馈中心 KPI 统计,对应后端 `FeedbackStats`。 */
export interface FeedbackStats {
  total: number;
  unresolved: number;
  /** 按分类计数(bug/feature/complaint/praise/support 等) */
  byCategory: Record<string, number>;
  /** 按状态计数 */
  byStatus: {
    open: number;
    inProgress: number;
    resolved: number;
    closed: number;
  };
  /** 未读数(readAt IS NULL) */
  unreadCount: number;
  /** 已分派数(assigneeAdminId IS NOT NULL) */
  assignedCount: number;
  /** 已解决数 */
  resolvedCount: number;
  /** 中位响应时长(分钟),无样本时 null */
  medianResponseMinutes: number | null;
  /** 近 7 日解决率(0-1),无样本时 null */
  resolveRate7d: number | null;
  /** CSAT 平均分(1-5),无样本时 null */
  csatAvg: number | null;
  /** CSAT 样本数 */
  csatCount: number;
  /** 近 7 日每日序列(旧→新,缺数据日为 0),驱动 KPI 卡 sparkline */
  responseSpark: number[];
  resolveSpark: number[];
  csatSpark: number[];
  /** 周环比(本周 vs 上周),数据不足时 null */
  responseDelta: number | null;
  resolveDelta: number | null;
  csatDelta: number | null;
}

/** m030:工单详情,对应后端 `FeedbackDetail`。 */
export interface FeedbackDetail {
  item: FeedbackItem;
  replies: FeedbackReply[];
  events: FeedbackEvent[];
  attachments: FeedbackAttachment[];
}
