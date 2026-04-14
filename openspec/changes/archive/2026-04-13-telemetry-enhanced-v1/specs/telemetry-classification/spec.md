## ADDED Requirements

### Requirement: Server classifies telemetry on receipt and stores structured summary
Upon successful validation and storage of a telemetry event, the server SHALL immediately extract and classify the payload into three structured categories and write one row to the `telemetry_summaries` table. This happens synchronously within the same request handler in `src/routes/telemetry.rs`, before the HTTP response is sent.

#### New table: `telemetry_summaries`
Added to `src/store/schema.rs`:
```sql
CREATE TABLE IF NOT EXISTS telemetry_summaries (
    id TEXT NOT NULL,                     -- same UUID as telemetry_events.id
    device_id TEXT NOT NULL,
    user_id TEXT,
    event_type TEXT NOT NULL,
    server_ts TEXT NOT NULL,
    -- Device profile (from payload.device, nullable when not provided)
    cpu_cores INTEGER,
    memory_gb REAL,
    screen_width INTEGER,
    screen_height INTEGER,
    pixel_ratio REAL,
    os_name TEXT,
    browser_name TEXT,
    browser_version TEXT,
    timezone TEXT,
    language TEXT,
    touch_support INTEGER,                -- 0/1
    online_status INTEGER,                -- 0/1
    -- Session stats (always present)
    session_duration_secs INTEGER NOT NULL DEFAULT 0,
    actions_per_min REAL NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    avg_response_time_ms REAL NOT NULL DEFAULT 0,
    -- Behavior summary (from payload.behavior, nullable)
    current_route TEXT,
    click_count INTEGER,
    click_targets_json TEXT,              -- JSON array of { label, tag }
    scroll_depth_pct REAL,
    visibility_changes INTEGER,
    route_changes INTEGER,
    -- Feature usage (from payload.featureUsage)
    feature_usage_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_telemetry_summaries_device
    ON telemetry_summaries(device_id, server_ts DESC);
```

#### Classification logic (in `src/routes/telemetry.rs`, after `insert_telemetry()`)
Extract fields from `body.payload` using `serde_json::Value` accessors:
- `device_profile`: read `payload["device"]` object; extract each field with type coercion; use `None` if field absent
- `session_stats`: read top-level `sessionDurationSecs`, `actionsPerMin`, `errorCount`, `avgResponseTimeMs`
- `behavior_summary`: read `payload["behavior"]` object; extract each field; serialize `clickTargets` array back to JSON string

When `payload["device"]` is absent (e.g., `periodic` events), all device columns in `telemetry_summaries` SHALL be written as NULL. The server MUST NOT look up or carry over device data from prior `session_start` events.

Both `insert_telemetry()` and `insert_telemetry_summary()` MUST execute within the **same database transaction**. If either write fails, the entire transaction is rolled back and the handler returns HTTP 500. The client is expected to retry on 500.

Call `state.store().insert_telemetry_summary(...)` with extracted values.

#### New store method: `insert_telemetry_summary`
Added to `src/store/operations/telemetry.rs`:
```rust
pub fn insert_telemetry_summary(
    &self,
    id: &str,
    device_id: &str,
    user_id: &str,
    event_type: &str,
    // device fields
    cpu_cores: Option<i64>,
    memory_gb: Option<f64>,
    screen_width: Option<i64>,
    screen_height: Option<i64>,
    pixel_ratio: Option<f64>,
    os_name: Option<&str>,
    browser_name: Option<&str>,
    browser_version: Option<&str>,
    timezone: Option<&str>,
    language: Option<&str>,
    touch_support: Option<bool>,
    online_status: Option<bool>,
    // session stats
    session_duration_secs: i64,
    actions_per_min: f64,
    error_count: i64,
    avg_response_time_ms: f64,
    // behavior
    current_route: Option<&str>,
    click_count: Option<i64>,
    click_targets_json: Option<&str>,
    scroll_depth_pct: Option<f64>,
    visibility_changes: Option<i64>,
    route_changes: Option<i64>,
    // feature usage
    feature_usage_json: &str,
) -> Result<(), StoreError>
```

#### New store method: `get_telemetry_summaries_by_device`
Returns `Vec<TelemetrySummary>` ordered by `server_ts DESC`, with `limit`/`offset` pagination. The struct mirrors all columns of `telemetry_summaries`.

#### Admin API change: `GET /api/admin/telemetry/:device_id`
The response body changes from returning raw `telemetry_events` records to returning `telemetry_summaries` rows. This removes the raw JSON blob from the admin view entirely.

Response schema per record:
```json
{
  "id": "<uuid>",
  "deviceId": "<string>",
  "userId": "<string | null>",
  "eventType": "periodic" | "session_start" | "on_demand",
  "serverTs": "<ISO8601>",
  "deviceProfile": {
    "cpuCores": <integer | null>,
    "memoryGb": <float | null>,
    "screenWidth": <integer | null>,
    "screenHeight": <integer | null>,
    "pixelRatio": <float | null>,
    "osName": "<string | null>",
    "browserName": "<string | null>",
    "browserVersion": "<string | null>",
    "timezone": "<string | null>",
    "language": "<string | null>",
    "touchSupport": <boolean | null>,
    "onlineStatus": <boolean | null>
  },
  "sessionStats": {
    "sessionDurationSecs": <integer>,
    "actionsPerMin": <float>,
    "errorCount": <integer>,
    "avgResponseTimeMs": <float>
  },
  "behaviorSummary": {
    "currentRoute": "<string | null>",
    "clickCount": <integer | null>,
    "clickTargets": [{ "label": "<string>", "tag": "<string>" }],
    "scrollDepthPct": <float | null>,
    "visibilityChanges": <integer | null>,
    "routeChanges": <integer | null>
  },
  "featureUsage": { "<feature>": <integer> }
}
```

#### Admin frontend: `ClientsPage.tsx` telemetry panel
The telemetry history panel in `frontend/src/pages/admin/ClientsPage.tsx` is updated to render structured data instead of `<pre>{JSON.stringify(payload)}</pre>`:

- **设备信息** 区块：OS / 浏览器 / CPU / 内存 / 分辨率 / 时区 / 语言
- **会话统计** 区块：会话时长 / 每分钟操作数 / 错误数 / 平均响应时间
- **行为摘要** 区块：当前路由 / 点击数 / 点击目标列表 / 滚动深度 / 路由跳转数 / 可见性变更数
- **功能使用** 区块：feature key → count 列表

Each block is conditionally rendered (hidden if all fields are null). The `TelemetryRecord` TypeScript interface in `frontend/src/api/admin.ts` is replaced with `TelemetrySummary` matching the new response schema.

#### Scenario: Telemetry with device object — all device fields populated
- **WHEN** client sends `session_start` with complete `device` object
- **THEN** `telemetry_summaries` row has all device columns populated
- **THEN** admin view shows OS, browser, CPU, memory, screen resolution

#### Scenario: Telemetry without device object — device fields null
- **WHEN** client sends `periodic` without `device` field
- **THEN** `telemetry_summaries` row has all device columns as NULL
- **THEN** admin view hides the device info block

#### Scenario: Admin views classified telemetry
- **WHEN** admin opens telemetry history for a device
- **THEN** each record shows structured blocks, not raw JSON
- **THEN** admin can see at a glance: device fingerprint, what page user was on, what was clicked

#### Scenario: Classification failure — transaction rolls back
- **WHEN** payload parsing for classification throws an error, OR `insert_telemetry_summary` fails
- **THEN** the entire transaction (including `telemetry_events` insert) is rolled back
- **THEN** the handler returns HTTP 500; the client retries on next interval
