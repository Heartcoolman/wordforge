## Context

现有遥测系统基于 `telemetry_events` 表存储原始 payload JSON，上报间隔 5 分钟，无心跳感知，管理后台直接渲染 JSON blob。

关键约束：
- 数据库使用 SQLite（单文件，连接池），迁移通过 `store/migrate.rs` 顺序执行
- SSE 连接通过 `AppState.active_sse: Arc<DashMap<String, Vec<SseClientInfo>>>` 追踪，key 为 device_id
- 后台任务已有先例（`rate_limit_cleanup_loop`、`WorkerManager`），通过 `tokio::spawn` 在 `main.rs` 启动
- 前端为 SolidJS，SSE 处理在 `api/client.ts` 的 `connectSseStream`，遥测在 `workers/telemetry.ts`

## Goals / Non-Goals

**Goals:**
- 上报间隔降至 5 秒；新增 `session_start` 事件携带设备指纹
- 服务端心跳看门狗：5 次连续丢包（≥25s）触发 `data_corrupted` SSE 事件
- 遥测入库时同步提取结构化摘要写入 `telemetry_summaries` 表
- 管理后台遥测详情展示分类摘要，不再渲染原始 blob
- 前端收到 `data_corrupted` 后展示全屏锁定弹窗，禁止所有交互

**Non-Goals:**
- 不修改 `telemetry_events` 表结构（向后兼容原始存储）
- 不改变认证、设备注册、ban/unban 流程
- 不对非 Web 客户端（iOS/macOS）做任何适配

## Decisions

**D1：心跳追踪存于内存，不写数据库**

在 `AppState` 新增 `last_heartbeat: Arc<DashMap<String, Instant>>`。遥测 `submit_telemetry` 收到请求时更新该 map；看门狗任务每 5 秒扫描 `active_sse` 的 key（有活跃 SSE 连接的设备），计算距上次心跳的秒数，≥25s 即推送 `data_corrupted`。

替代方案：将丢包计数写入数据库——引入不必要的 I/O，且 Instant 足够精确，进程重启后 SSE 连接也会断开，内存状态天然重置。

**D2：`telemetry_summaries` 与 `telemetry_events` 并存，不替换**

摘要表存储结构化字段（device_profile / session_stats / behavior_summary），均为 TEXT 或 REAL 类型，方便查询。原始 `telemetry_events` 保持不变。入库时在同一个事务中同步写入摘要。

替代方案：将摘要字段加列到 `telemetry_events`——污染原有表，且不适合 `periodic` 事件（无 device 字段时大量列为 NULL）。

**D3：看门狗为独立 Tokio 任务，与 `rate_limit_cleanup_loop` 模式一致**

在 `main.rs` 使用 `tokio::spawn` 启动，传入 `state.clone()` 和 `shutdown_rx`。任务内循环 `tokio::time::interval(5s)`，每 tick 遍历 `active_sse` keys。

**D4：`DataCorrupted` 作为新的 `SseEvent` 枚举变体**

`SseEvent` 枚举新增 `DataCorrupted` 变体，`realtime.rs` 的 `event_name` match 补充对应分支 `"data_corrupted"`。

**D5：前端锁定组件为独立文件，通过全局信号控制显示**

新增 `SystemLockedModal.tsx`（SolidJS 全屏遮罩），在 `App.tsx` `MaintenanceProvider` 中通过 `createSignal` 控制显示，收到 `data_corrupted` SSE 事件时设为 true。

## Risks / Trade-offs

- **[Risk] 5 秒间隔大幅增加请求量**：每个活跃设备每分钟 12 次请求，对 SQLite 写入压力显著上升。→ Mitigation：`periodic` 事件仅在有实质行为变化时写入摘要（behavior 全零则跳过摘要写入，仍写 `telemetry_events` 以保留心跳记录）。
- **[Risk] 看门狗误判**：网络抖动可能导致合法客户端被触发 `data_corrupted`。→ Mitigation：阈值设为 5 次（25s），而非 1 次；客户端 UI 仅锁定不断连，用户手动重启即可恢复。
- **[Risk] `last_heartbeat` map 内存泄漏**：设备断开 SSE 后，map 中的条目不再被清理。→ Mitigation：看门狗扫描时，若 device_id 不再存在于 `active_sse`，则从 `last_heartbeat` 移除。

## Migration Plan

1. 新增 migration `003_telemetry_enhanced`，创建 `telemetry_summaries` 表
2. 后端代码变更：`state.rs`、`telemetry.rs`、`store/operations/telemetry.rs`、`store/schema.rs`、`store/migrate.rs`、`main.rs`、`routes/admin/clients.rs`
3. 前端代码变更：`workers/telemetry.ts`、`lib/device.ts`（新增）、`App.tsx`、`components/SystemLockedModal.tsx`（新增）、`pages/admin/ClientsPage.tsx`
4. 部署：单次滚动发布，无需停机；旧客户端缺少 `device` 字段时服务端写 NULL，不报错

**Rollback**：回滚后 `telemetry_summaries` 表保留（无害），旧代码不查询该表；`last_heartbeat` 为内存态，进程重启自动清除。

## Open Questions

- `periodic` 事件中 `device` 字段是否允许客户端重复携带（用于更新指纹）？→ 当前设计：允许，若存在则覆盖摘要中的 device_profile
- `telemetry_summaries` 是否需要保留历史（多行 per device）还是 upsert（单行最新）？→ 当前设计：多行，与 `telemetry_events` 一一对应，通过 `telemetry_event_id` 关联；管理后台取最新一条展示
