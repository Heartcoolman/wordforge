/**
 * 数据探针看板（probe-telemetry）类型契约。
 *
 * 与 `src/routes/admin/probe_telemetry.rs` + `src/store/operations/probe_telemetry.rs`
 * 对齐（camelCase）。响应外层 ok() 包装在 http.ts 已自动 unwrap，这里只描述 data。
 */

/** KPI：计数型（活跃探针 / 24h 事件 / 队列积压）。 */
export interface KpiCount {
  value: number;
  /** 活跃探针带 total（总定义探针数）；其余省略。 */
  total?: number;
  /** events24h 带 deltaPct；前一窗口为 0 时被后端省略（无可比基准）。 */
  deltaPct?: number;
  note?: string;
}

/** KPI：比率型（采集错误率，0~1 小数）。 */
export interface KpiRate {
  value: number;
  note?: string;
}

export interface ProbeOverview {
  generatedAt: string;
  activeProbes: KpiCount;
  events24h: KpiCount;
  queueBacklog: KpiCount;
  collectErrorRate: KpiRate;
}

/** 单个派生探针行。 */
export interface ProbeRow {
  key: string;
  label: string;
  desc: string;
  count24h: number;
  eps: number;
  lastTs: string | null;
  sampleRate: number;
  locked: boolean;
  enabled: boolean;
  /** 仅 click 探针为 "periodic"（绑定真实 telemetry event_type）；其余省略 → 滑杆 disabled。 */
  samplingEventType?: string;
}

export interface ProbeGroup {
  group: string;
  hue: number;
  probes: ProbeRow[];
}

export interface ProbesResponse {
  generatedAt: string;
  groups: ProbeGroup[];
}

/** 采样规则行（probe_sampling_config）。 */
export interface SamplingRule {
  eventType: string;
  sampleRate: number;
  enabled: boolean;
  locked: boolean;
  priority: number;
  /** 该 telemetry event_type 在看板上绑定的派生探针 key（无则省略）。 */
  target?: string;
}

export interface SamplingResponse {
  globalDefault: number;
  rules: SamplingRule[];
}

/** PATCH /sampling/:event_type 成功返回。 */
export interface SamplingPatchResult {
  eventType: string;
  sampleRate: number;
  enabled: boolean;
  locked: boolean;
  updatedAt: string;
}

/** 真实 SQLite sink 表状态。 */
export interface SinkStatus {
  id: string;
  label: string;
  kind: string;
  rowCount: number;
  lastWriteTs: string | null;
  /** 无清理 → 恒 null（前端显示"永久/不限"）。 */
  retentionDays: number | null;
  lagSecs: number;
}

export interface SinksResponse {
  generatedAt: string;
  sinks: SinkStatus[];
}

/** schema 采样：无数据时 sampledAt/payload 为 null。 */
export interface SchemaSample {
  eventType: string;
  sampledAt: string | null;
  payload: unknown;
}

/** 采样改动审计行。action ∈ add/mod/pause/del。 */
export interface AuditRow {
  ts: string;
  action: string;
  eventType: string;
  oldValue: string | null;
  newValue: string | null;
  adminId: string | null;
}

export interface AuditResponse {
  rows: AuditRow[];
}

/** 设备摘要（后端从 payload.device 解析；字段缺失即省略）。 */
export interface StreamDevice {
  os?: string;
  model?: string;
  online?: boolean;
  language?: string;
}

/** payload 标量指标（key 原样，前端贴中文标签）。 */
export interface StreamMetric {
  key: string;
  value: string;
}

/** 实时事件流单条（后端已 humanize：设备摘要 + 关键指标 + 原始 payload）。 */
export interface StreamEvent {
  id: string;
  ts: string;
  type: string;
  deviceId: string;
  /** 无 device 字段时省略。 */
  device?: StreamDevice;
  /** 顶层 + behavior.* 标量字段（最多 8 条）。 */
  metrics: StreamMetric[];
  /** 完整原始 payload（"原始 JSON"展开用）。 */
  payloadRaw: string;
}

export interface StreamResponse {
  events: StreamEvent[];
}
