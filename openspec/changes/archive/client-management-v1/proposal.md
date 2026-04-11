## Why

产品需要对多端客户端（iOS/macOS/Web/iPadOS）进行运营管控：在服务器维护期间强制锁定所有客户端界面；实时感知哪些设备在线；独立封禁设备（区别于账号封禁）；收集客户端遥测数据以识别滥用行为。

## What Changes

- 新增维护模式中间件，维护期间所有非管理员 API 返回 503，`GET /api/status` 公开端点返回维护状态
- 新增 SSE 维护事件推送（`maintenance` 事件），客户端实时感知并锁定界面
- 新增设备指纹中间件，通过 `X-Device-Id` / `X-Device-Platform` 请求头追踪客户端身份
- 新增独立的设备封禁机制（不复用现有用户 `is_banned` 字段）
- 新增 Admin 客户端管理 API（在线列表、封禁/解封）
- 新增遥测系统：客户端定期主动上报 + 管理员按需 SSE 拉取
- 新增前端维护界面、设备 ID 管理库、Admin Clients 页面、遥测 worker

## Capabilities

### New Capabilities

- `maintenance-mode`: 维护模式双轨感知——HTTP 中间件 503 拦截 + SSE `maintenance` 事件推送，前端锁定界面
- `client-management`: 设备指纹注册、在线状态追踪（SSE 实时 + 15 分钟活跃）、独立设备封禁
- `client-telemetry`: 客户端行为遥测——周期性主动上报（5 分钟）+ 管理员 SSE 按需触发拉取

### Modified Capabilities

（无现有 spec 级别行为变更）

## Impact

**后端**：新增中间件 `maintenance.rs` / `device.rs`；新增 `client_devices` / `telemetry_events` 表；扩展 `AppState`（`maintenance_tx: broadcast::Sender<bool>`、`active_sse: Arc<DashMap<String, SseClientInfo>>`）；扩展 SSE handler；新增 admin 路由 `/api/admin/clients`、`/api/admin/telemetry/:device_id`；新增公开端点 `GET /api/status`、认证端点 `POST /api/telemetry`。

**前端**：修改 `client.ts` 注入设备头；新增维护界面 `MaintenancePage`；新增 Admin Clients 页面；新增遥测 worker；修改 `App.tsx` 路由逻辑。

**数据库**：新增两张表（`client_devices`、`telemetry_events`），通过 `migrate.rs` 迁移，不影响现有表结构。
