# OpenSpec Proposal: 客户端管理与运营安全体系

**ID**: client-management-v1  
**Date**: 2026-04-11  
**Status**: PROPOSED

---

## 1. 需求原文

> 1. 维护功能：管理员在后端打开了此开关，则其他前端（客户端，如 iOS/macOS/Web/iPadOS 等）通过接口收到"服务器维护中"的界面，然后强制停在此界面。
> 2. 管理员后台要能看到现在哪个客户端在线，并且要支持封禁客户端（和封禁用户分开，是两个功能）。
> 3. 后端预留接口，或者主动向客户端索取相应的遥测数据，以此来判断和识别用户的行为，杜绝滥用。

用户确认选项：

| 功能点 | 选择 |
|--------|------|
| 维护模式感知 | 双轨并行：轮询端点 + SSE 推送 |
| 客户端身份 | 平台 + 指纹（X-Device-Id UUID + X-Device-Platform） |
| 在线定义 | 两者都显示：SSE 实时连接 + 15 分钟内活跃 |
| 遥测方式 | 两者都要：客户端定期主动上报 + 服务端按需拉取 |

---

## 2. 约束集

### 硬约束

1. 数据库为 SQLite（rusqlite），新增表须通过 `src/store/schema.rs` DDL + `src/store/migrate.rs` 迁移。
2. `maintenance_mode` 字段已在 `system_settings` 表中存在，admin 设置路由已实现读写；**不得改变现有字段或破坏现有逻辑**。
3. 现有 `is_banned` 字段为用户封禁；客户端封禁必须独立实现，不复用此字段。
4. SSE 处理位于 `src/routes/realtime.rs`，当前仅推送 `amas_state` 事件；需扩展支持新事件类型，不删除现有逻辑。
5. 所有 API 遵循现有 `ok()` / `AppError` / `JsonBody` 的 response 模式。
6. Admin 路由挂载于 `/api/admin/`，受 `AdminAuthUser` 提取器保护。
7. 用户路由受 `AuthUser` 提取器保护（JWT），`/api/status` 为无需认证的公开端点。
8. 前端使用 SolidJS + `@solidjs/router`；API 调用通过 `frontend/src/api/client.ts` 的 `api` 对象。

### 软约束

- 代码精简，无冗余；注释非必要不写。
- 新增文件严格遵循现有模块结构（admin 路由放 `src/routes/admin/`，store 操作放 `src/store/operations/`）。
- 中间件在 `src/middleware/` 下。

---

## 3. 成功判据

| # | 可验证行为 |
|---|-----------|
| M1 | 管理员切换维护模式后，所有非管理员 API 返回 503，`GET /api/status` 返回 `maintenanceMode: true` |
| M2 | 已建立 SSE 连接的客户端在 5 秒内收到 `maintenance` 事件 |
| M3 | 客户端收到维护信号后锁定在维护界面，无法导航至其他页面 |
| C1 | 管理员控制台可见所有活跃 SSE 客户端列表（device_id, platform, user_id, 连接时长） |
| C2 | 管理员控制台可见过去 15 分钟内有 API 活动的非 SSE 客户端 |
| C3 | 封禁客户端后，该 device_id 的所有后续请求返回 403（即使持有有效 JWT） |
| C4 | 客户端封禁不影响同账号其他设备；用户封禁不影响用户在其他设备的 device_id 状态 |
| T1 | 客户端每 5 分钟自动向 `POST /api/telemetry` 上报行为摘要 |
| T2 | 管理员可对指定 device_id 触发即时遥测拉取，客户端收到 SSE `telemetry_request` 后立即上报 |
| T3 | 管理员在 Clients 页面可查看每个客户端的遥测历史 |

---

## 4. 技术方案

### 4.1 客户端身份（平台 + 指纹）

**定义**：客户端在首次运行时生成持久化 UUID 作为 `device_id`，与 `platform` 字段（`web`/`ios`/`macos`/`ipados`）组合构成客户端身份。

**传输机制**：每次 API 请求通过 HTTP 请求头携带：
```
X-Device-Id: <uuid>
X-Device-Platform: web
```

Web 端 `device_id` 存储于 `localStorage`；原生客户端存储于系统密钥链。

**注册逻辑**：后端在接收到携带 `X-Device-Id` 的已认证请求时，自动在 `client_devices` 表中 upsert 设备记录（首次注册 + 每次更新 `last_seen_at`）。

---

### 4.2 数据库变更

#### 新增表：`client_devices`

```sql
CREATE TABLE IF NOT EXISTS client_devices (
    device_id TEXT NOT NULL,
    platform TEXT NOT NULL DEFAULT 'unknown',
    user_id TEXT,
    first_seen_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    is_banned INTEGER NOT NULL DEFAULT 0 CHECK (is_banned IN (0, 1)),
    banned_at TEXT DEFAULT NULL,
    banned_by TEXT DEFAULT NULL,
    ban_reason TEXT DEFAULT NULL,
    PRIMARY KEY (device_id)
);
CREATE INDEX IF NOT EXISTS idx_client_devices_user ON client_devices(user_id, last_seen_at DESC);
CREATE INDEX IF NOT EXISTS idx_client_devices_active ON client_devices(last_seen_at DESC) WHERE is_banned = 0;
```

#### 新增表：`telemetry_events`

```sql
CREATE TABLE IF NOT EXISTS telemetry_events (
    id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    user_id TEXT,
    event_type TEXT NOT NULL DEFAULT 'periodic',
    payload_json TEXT NOT NULL DEFAULT '{}',
    client_ts TEXT NOT NULL,
    server_ts TEXT NOT NULL,
    PRIMARY KEY (id)
);
CREATE INDEX IF NOT EXISTS idx_telemetry_device ON telemetry_events(device_id, server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_telemetry_server_ts ON telemetry_events(server_ts DESC);
```

---

### 4.3 后端：Feature 1 — 维护模式双轨

#### 新增中间件：`src/middleware/maintenance.rs`

- 拦截所有非 `/api/admin/` 且非 `/api/status` 且非 `/health` 的路由。
- 从 `AppState` 读取 `system_settings.maintenance_mode`（可缓存于 `AtomicBool` 避免频繁 DB 读取）。
- 若为 `true`，返回 `503 Service Unavailable`，JSON body：`{ "code": "MAINTENANCE", "message": "服务器维护中，请稍后重试" }`。

#### 新增公开端点：`GET /api/status`

无需认证。返回：
```json
{
  "maintenanceMode": false,
  "version": "0.x.x"
}
```

#### SSE 维护事件推送

当管理员通过 `PUT /api/admin/settings` 修改 `maintenance_mode` 时：
- 后端通过 `tokio::sync::broadcast` 广播 `MaintenanceChanged(bool)` 事件。
- SSE handler 订阅此频道，收到事件后向客户端推送：
  ```json
  { "type": "maintenance", "active": true }
  ```

在 `AppState` 中新增 `maintenance_tx: broadcast::Sender<bool>` 字段。

---

### 4.4 后端：Feature 2 — 客户端在线状态与封禁

#### 中间件：`src/middleware/device.rs`

- 提取 `X-Device-Id` 和 `X-Device-Platform` 请求头。
- 若 `X-Device-Id` 存在，检查 `client_devices` 表：
  - 若 `is_banned = 1`，返回 403：`{ "code": "CLIENT_BANNED", "message": "设备已被封禁" }`。
  - 否则 upsert 设备记录（更新 `last_seen_at` 和 `user_id`）。
- 在 SSE handler 中将 device 信息注入，用于活跃连接追踪。

#### AppState 新增字段

```rust
// 活跃 SSE 连接：device_id → (user_id, platform, connected_at)
active_sse: Arc<DashMap<String, SseClientInfo>>
```

> 使用 `dashmap` crate（已在生态系统中广泛使用，类型安全的并发 HashMap）。

#### 新增 Admin 路由：`src/routes/admin/clients.rs`

```
GET  /api/admin/clients              # 列表：SSE 在线 + 15 分钟内活跃
POST /api/admin/clients/:id/ban      # 封禁客户端
POST /api/admin/clients/:id/unban    # 解封客户端
POST /api/admin/clients/:id/request-telemetry  # 触发即时遥测拉取
```

`GET /api/admin/clients` 响应结构：
```json
{
  "sseLive": [
    { "deviceId": "...", "platform": "web", "userId": "...", "connectedSecs": 120 }
  ],
  "recentlyActive": [
    { "deviceId": "...", "platform": "ios", "userId": "...", "lastSeenAt": "..." }
  ]
}
```

---

### 4.5 后端：Feature 3 — 遥测

#### 客户端主动上报：`POST /api/telemetry`

需 `AuthUser` 认证。请求体：
```json
{
  "eventType": "periodic",
  "clientTs": "2026-04-11T10:00:00Z",
  "payload": {
    "sessionDurationSecs": 300,
    "actionsPerMin": 12.5,
    "featureUsage": { "flashcard": 45, "vocabulary": 12 },
    "errorCount": 0,
    "avgResponseTimeMs": 234
  }
}
```

后端存入 `telemetry_events` 表，关联 `device_id`（从请求头提取）和 `user_id`（从 JWT 提取）。

#### 服务端按需拉取

管理员调用 `POST /api/admin/clients/:id/request-telemetry` 后：
- 后端通过 `AppState.active_sse` 找到该 device 的 SSE 连接。
- 通过已有的 broadcast 机制向该连接推送：
  ```json
  { "type": "telemetry_request", "requestId": "..." }
  ```
- 客户端收到后立即调用 `POST /api/telemetry`（携带 `eventType: "on_demand"`）。

#### Admin 遥测查询：`GET /api/admin/telemetry/:device_id`

返回该设备最近 50 条遥测记录。

---

## 5. 前端变更

### 5.1 客户端公共层（`frontend/src/lib/`）

- 新增 `device.ts`：首次访问时生成并持久化 `device_id`（`localStorage`），导出 `getDeviceId()` 和 `getDevicePlatform()` 函数。
- 修改 `frontend/src/api/client.ts`：在所有请求中自动注入 `X-Device-Id` 和 `X-Device-Platform` 请求头。

### 5.2 维护模式 UI

- 新增 `frontend/src/pages/MaintenancePage.tsx`：全屏维护提示界面，不含任何导航元素。
- 修改 `frontend/src/App.tsx`：
  - 应用启动时调用 `GET /api/status` 检查维护状态。
  - 每 30 秒轮询一次（fallback）。
  - SSE 收到 `maintenance` 事件时立即更新状态。
  - 维护状态激活时，所有路由渲染被 `MaintenancePage` 替换，不可导航。
  - 维护结束后自动恢复（不需要用户手动刷新）。

### 5.3 Admin：新增 Clients 页面

- 新增 `frontend/src/pages/admin/ClientsPage.tsx`：
  - 分 "SSE 实时连接" 和 "近期活跃" 两个标签列表展示客户端。
  - 每行显示：device_id（截断+复制）、平台、关联用户、状态、操作（封禁/解封/拉取遥测）。
  - 封禁确认弹窗（复用现有 `UserManagementPage.tsx` 的确认模式）。
  - 点击设备行展开遥测历史记录。
- 修改 `frontend/src/App.tsx`：在 admin 路由中注册 `/admin/clients` → `ClientsPage`。
- 修改 `frontend/src/components/layout/AdminLayout.tsx`：在侧边栏新增"客户端管理"入口。

### 5.4 遥测上报（客户端）

- 新增 `frontend/src/workers/telemetry.ts`：封装定时上报逻辑，每 5 分钟调用 `POST /api/telemetry`，收集页面行为摘要（会话时长、功能使用计数）。
- 在 `main.tsx` 或 `App.tsx` 中初始化此 worker（仅登录态激活）。

---

## 6. 变更文件清单

### 新增文件

| 文件路径 | 说明 |
|----------|------|
| `src/middleware/maintenance.rs` | 维护模式拦截中间件 |
| `src/middleware/device.rs` | 设备指纹提取与封禁检查中间件 |
| `src/routes/admin/clients.rs` | 客户端管理 admin 路由 |
| `src/store/operations/clients.rs` | client_devices 表 CRUD |
| `src/store/operations/telemetry.rs` | telemetry_events 表 CRUD |
| `frontend/src/lib/device.ts` | 设备 ID 生成与持久化 |
| `frontend/src/pages/MaintenancePage.tsx` | 维护界面 |
| `frontend/src/pages/admin/ClientsPage.tsx` | 客户端管理界面 |
| `frontend/src/workers/telemetry.ts` | 遥测上报 worker |

### 修改文件

| 文件路径 | 变更摘要 |
|----------|---------|
| `src/store/schema.rs` | 新增 `client_devices`、`telemetry_events` 表 DDL |
| `src/store/migrate.rs` | 新增迁移步骤 |
| `src/store/operations/mod.rs` | 导出新模块 |
| `src/state.rs` | 新增 `maintenance_tx`、`active_sse` 字段 |
| `src/routes/admin/mod.rs` | 挂载 `/clients` 路由 |
| `src/routes/realtime.rs` | 订阅 maintenance/telemetry 广播，追踪 SSE 连接到 active_sse |
| `src/routes/mod.rs` / `src/main.rs` | 挂载 maintenance 中间件、`/api/status`、`/api/telemetry` |
| `src/middleware/mod.rs` | 导出新中间件 |
| `frontend/src/api/admin.ts` | 新增 clients 和 telemetry API 方法 |
| `frontend/src/api/client.ts` | 注入 X-Device-Id/X-Device-Platform 请求头 |
| `frontend/src/App.tsx` | 状态检查轮询、维护界面路由拦截、注册 ClientsPage |
| `frontend/src/components/layout/AdminLayout.tsx` | 侧边栏新增"客户端管理"链接 |

---

## 7. 风险与开放问题

| 风险 | 缓解措施 |
|------|---------|
| `dashmap` 内存消耗（大量 SSE 连接） | 连接断开时立即从 `active_sse` 删除（利用现有 `SseGuard` Drop 模式） |
| device_id 可被伪造（web 端） | 仅作为软性标识用于行为监控，不替代账号级安全；封禁决策由管理员人工触发 |
| 遥测数据量 | 仅存聚合摘要（非原始事件流），`telemetry_events` 表按时间分页查询 |
| SSE 按需拉取时客户端已断线 | 后端检查 `active_sse` 是否有该 device 的活跃连接，若无则返回错误提示管理员 |
| 迁移兼容性 | 新增表不影响现有表；迁移脚本需在 `schema_version` 递增后运行 |
