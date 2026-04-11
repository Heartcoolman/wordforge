## Why

当服务器更新（版本升级、新增学习内容/词书、新增功能如遥测上报等），已连接的客户端仍在运行旧版前端，无法使用新功能或获取新内容。需要一个实时推送机制，通知客户端"服务端有更新"，引导用户刷新页面获取最新版本。

## What Changes

- 新增专用 `UpdatePayload` 结构体（不修改现有 `SseEvent` 枚举）
- `AppState` 新增 `update_tx: broadcast::Sender<UpdatePayload>` 广播通道
- SSE handler 监听 `update_tx`，向所有已连接客户端推送 `update_available` 事件
- 新增 Admin API `POST /api/admin/broadcast-update`，管理员可手动触发更新广播（用于新内容/新功能上线）
- 前端 `/api/status` 轮询（已有，每 30 秒）增加版本对比逻辑：首次加载记录服务端版本，后续轮询发现版本变化时自动触发更新提示
- 前端新增 `UpdateBanner` 组件——可关闭的顶部提示横幅，点击可刷新页面
- `SseCallbacks` 新增 `onUpdateAvailable` 回调，处理 SSE `update_available` 事件
- 管理后台设置页新增"更新通知"触发按钮

## Capabilities

### New Capabilities

- `update-notification`: 双轨更新检测——SSE `update_available` 事件实时推送 + `/api/status` 版本轮询对比。前端展示可关闭的提示横幅，用户可选择立即刷新或稍后处理。

### Modified Capabilities

- `realtime` (SSE handler)：新增 `update_available` 事件监听分支，使用独立 `broadcast::Receiver<UpdatePayload>`
- `admin` (管理后台)：新增广播更新通知的 API 端点和前端触发按钮

## Impact

**后端**（`src/` 目录下修改）：
- `src/state.rs`：新增 `UpdatePayload` 结构体；`AppState` 新增 `update_tx: broadcast::Sender<UpdatePayload>` 通道和 `update_rx()`、`broadcast_update()` 方法
- `src/routes/realtime.rs`：SSE stream 的 `tokio::select!` 新增 `update_rx.recv()` 分支
- `src/routes/admin/mod.rs`：注册新的广播路由
- 新增 `src/routes/admin/broadcast.rs`：`POST /api/admin/broadcast-update` 端点

**前端**（`frontend/src/` 目录下修改）：
- `frontend/src/api/client.ts`：新增 `updateInfo` signal；`SseCallbacks` 新增 `onUpdateAvailable` 回调；`connectSseStream` 处理 `update_available` 事件
- `frontend/src/api/admin.ts`：新增 `broadcastUpdate` API 方法
- `frontend/src/App.tsx`：`MaintenanceProvider` 内的 `/api/status` 轮询增加版本对比逻辑；接入 `UpdateBanner` 组件
- 新增 `frontend/src/components/ui/UpdateBanner.tsx`：可关闭的顶部提示横幅组件
- `frontend/src/pages/admin/SettingsPage.tsx`：新增更新通知触发卡片

**数据库**：无变更。
