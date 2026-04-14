## Why

现有遥测系统存在三项根本性缺陷：

1. **识别粒度不足**：`X-Device-Id` 仅是 localStorage UUID，无任何硬件特征支撑，同一设备清空 Storage 即可伪造新身份；payload 仅包含 5 个统计量，无法判断用户在做什么。
2. **无实时感知**：5 分钟上报间隔无法区分"客户端正常运行"与"客户端已崩溃/卡死/被篡改"；管理员对设备存活状态完全盲目。
3. **原始数据直接暴露**：管理后台将 JSON blob 直呈管理员，无任何分类汇总，认知负担极高。

本 spec 一次性解决上述三项问题：细化遥测 payload（以唯一性为第一优先级，行为为第二）；增加 5 秒心跳 + 服务端看门狗；服务端分类处理后再展示；客户端连接异常时强制锁定。

## What Changes

- 遥测上报间隔从 **5 分钟** 改为 **5 秒**；payload 拆分为 `device` 静态指纹 + `behavior` 行为增量
- 新增服务端 **心跳看门狗**：监控每台有活跃 SSE 连接的设备，5 次连续丢包（≥25 秒）后推送 `data_corrupted` SSE 事件
- 新增 **遥测分类处理**：服务端收到上报后立即提取 `device_profile` / `session_stats` / `behavior_summary` 三类结构化字段并写入 `telemetry_summaries` 表；管理后台查询该表，不再直接看 raw payload
- 新增 **客户端锁定**：前端收到 `data_corrupted` SSE 事件后，展示全屏不可关闭弹窗"数据损坏，请重启客户端"，同时禁止所有交互

## Capabilities

### New Capabilities

- `telemetry-payload`: 增强型遥测 payload——首次上报附带设备指纹（CPU/内存/屏幕/OS/浏览器/时区等），后续每次上报携带行为增量（路由、点击、滚动深度、可见性变更）
- `heartbeat-watchdog`: 服务端后台任务，每 5 秒扫描一次所有有活跃 SSE 连接的设备，5 次连续未收到遥测数据即推送 `data_corrupted` 事件
- `telemetry-classification`: 遥测入库前自动分类：提取结构化 `device_profile`、`session_stats`、`behavior_summary`，写入独立的 `telemetry_summaries` 表；管理后台遥测详情页展示分类结果，不展示原始 blob
- `client-lockdown`: 前端 SSE handler 新增 `data_corrupted` 事件处理：弹出全屏模态框，锁定所有用户操作，提示重启

### Modified Capabilities

- `client-telemetry`（来自 `client-management-v1`）: 上报间隔由 5 分钟改为 5 秒；`eventType` 新增 `session_start`；payload schema 扩展为包含 `device` 和 `behavior` 子对象

## Impact

**后端**：
- `src/routes/telemetry.rs`：扩展 `TelemetryRequest` 结构体接收新 payload schema；新增分类处理逻辑（调用 store 方法写入 `telemetry_summaries`）
- `src/store/operations/telemetry.rs`：新增 `insert_telemetry_summary` / `get_telemetry_summaries_by_device` 方法
- `src/store/schema.rs`：新增 `telemetry_summaries` 表
- `src/store/migrate.rs`：添加迁移步骤
- `src/state.rs`：`AppState` 新增 `last_heartbeat: Arc<DashMap<String, Instant>>`；新增 `heartbeat_miss_count: Arc<DashMap<String, u8>>`
- `src/state.rs`（SseEvent）：新增 `DataCorrupted` 枚举变体
- 新增后台 Tokio 任务（在 main.rs 启动时 spawn）：每 5 秒执行心跳扫描
- `src/routes/admin/clients.rs`：`get_telemetry` 路由改为查询 `telemetry_summaries`；增加新的管理端摘要接口

**前端**：
- `frontend/src/workers/telemetry.ts`：`INTERVAL_MS` 改为 5000；新增 `device` 指纹采集（`collectDeviceInfo()`）；新增 `behavior` 增量追踪（`trackClick`, `trackRoute`, `trackScroll`, `trackVisibility`）；`session_start` 事件在 `startTelemetryWorker` 时立即上报一次
- `frontend/src/lib/device.ts`：新增 `collectDeviceFingerprint()` 返回硬件/软件静态信息
- `frontend/src/App.tsx`：新增 `data_corrupted` SSE 事件处理；挂载全屏锁定组件
- 新增 `frontend/src/components/SystemLockedModal.tsx`：全屏遮罩 + 不可关闭弹窗
- `frontend/src/pages/admin/ClientsPage.tsx`：遥测历史面板改为展示分类数据（`device_profile` / `session_stats` / `behavior_summary`），不再渲染 JSON blob

**数据库**：
- 新增 `telemetry_summaries` 表（见 spec `telemetry-classification`）

## 客户端需要接入的新 API

> 以下是本次变更中**客户端（iOS / macOS / Web / iPadOS）新增或变更的接入点**，仅列出非 Web 前端已内联实现的部分。

### 1. `POST /api/telemetry`（变更：payload schema 扩展）

原有端点保持不变，**payload 字段扩展**如下：

```json
{
  "eventType": "periodic" | "on_demand" | "session_start",
  "requestId": "<uuid> | null",
  "clientTs": "<ISO8601 UTC>",
  "payload": {
    "device": {                        // 仅 session_start 时必填；periodic 可省略或重复
      "cpuCores": <integer>,
      "memoryGb": <float | null>,      // navigator.deviceMemory，不支持则 null
      "screenWidth": <integer>,
      "screenHeight": <integer>,
      "pixelRatio": <float>,
      "osName": "<string>",            // e.g. "iOS 17.4", "macOS 14.3"
      "browserName": "<string>",       // e.g. "Safari", "Chrome"
      "browserVersion": "<string>",
      "timezone": "<string>",          // e.g. "Asia/Shanghai"
      "language": "<string>",          // e.g. "zh-CN"
      "touchSupport": <boolean>,
      "onlineStatus": <boolean>
    },
    "behavior": {                      // 每次上报的行为增量（自上次上报以来）
      "currentRoute": "<string>",      // 当前页面路径，e.g. "/learn"
      "clickCount": <integer>,
      "clickTargets": [                // 最近 N 次点击，N ≤ 20
        { "label": "<string>", "tag": "<string>" }
      ],
      "scrollDepthPct": <float>,       // 0.0–100.0，当前页最大滚动百分比
      "visibilityChanges": <integer>,  // Tab 进入/离开焦点次数
      "routeChanges": <integer>        // 路由跳转次数
    },
    "sessionDurationSecs": <integer ≥ 0>,
    "actionsPerMin": <float ≥ 0>,
    "featureUsage": { "<feature>": <integer ≥ 0> },
    "errorCount": <integer ≥ 0>,
    "avgResponseTimeMs": <float ≥ 0>
  }
}
```

- `session_start` 事件：客户端建立 SSE 连接后立即发送一次，**必须**携带完整 `device` 对象
- `periodic` 事件：每 **5 秒**发送一次，`device` 可省略；`device` 缺失时，`telemetry_summaries` 该行的所有 device 列写入 NULL
- 原有字段（`sessionDurationSecs` 等）保持向后兼容，服务端仍接受

### 2. SSE 新事件类型 `data_corrupted`（新增）

```json
{ "type": "data_corrupted" }
```

- **触发条件**：服务端连续 5 次（25 秒）未收到该设备的遥测心跳
- **客户端必须实现**：
  1. 展示全屏不可关闭弹窗：标题"数据损坏"，正文"客户端数据已损坏，请重启应用后再试。"
  2. 禁止所有用户交互（不可导航、不可点击、不可关闭弹窗）
  3. SSE 连接与 telemetry worker 继续运行（UI 仍锁定）；用户刷新/重启应用后锁定状态自然解除

> **注意**：`data_corrupted` 事件本身不附带任何额外字段；若设备持续无心跳，服务端每 25 秒重复发送一次；客户端 UI 保持锁定直到人工重启。
