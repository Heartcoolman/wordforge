## 1. 数据库迁移

- [x] 1.1 在 `src/store/schema.rs` 中添加 `telemetry_summaries` 表 DDL（含索引）
- [x] 1.2 在 `src/store/migrate.rs` 中新增 `m003_telemetry_enhanced` 迁移函数，创建 `telemetry_summaries` 表

## 2. Store 层新增方法

- [x] 2.1 在 `src/store/operations/telemetry.rs` 中添加 `TelemetrySummary` 结构体（对应表所有列）
- [x] 2.2 在 `src/store/operations/telemetry.rs` 中实现 `insert_telemetry_and_summary()`（事务）
- [x] 2.3 在 `src/store/operations/telemetry.rs` 中实现 `get_telemetry_summaries_by_device()`（带分页）

## 3. AppState 扩展

- [x] 3.1 在 `src/state.rs` `SseEvent` 枚举中添加 `DataCorrupted` 变体
- [x] 3.2 在 `src/state.rs` `AppState` 中添加 `last_heartbeat` 和 `heartbeat_miss_count` 字段
- [x] 3.3 在 `AppState::new()` 中初始化这两个字段，暴露对应访问器方法

## 4. 遥测路由更新

- [x] 4.1 在 `src/routes/telemetry.rs` 中将两次写入合并到同一事务（insert_telemetry_and_summary）
- [x] 4.2 在 `submit_telemetry` 中提取 payload 字段（device / behavior / session stats）并调用分类写入
- [x] 4.3 在 `submit_telemetry` 成功后更新 `last_heartbeat` 和重置 `heartbeat_miss_count`

## 5. 心跳看门狗

- [x] 5.1 新建 `src/workers/heartbeat_watchdog.rs`，实现 5 秒扫描循环：检测 miss 次数 ≥5 时发送 `SseEvent::DataCorrupted`
- [x] 5.2 在 SSE 连接建立时（`src/routes/realtime.rs`）初始化 `last_heartbeat[device_id]`
- [x] 5.3 在 `src/routes/realtime.rs` `event_name` match 中补充 `DataCorrupted => "data_corrupted"` 分支
- [x] 5.4 在 `src/main.rs` 中 `tokio::spawn` 启动看门狗任务（传入 state.clone() 和 shutdown_rx）

## 6. 管理后台 API 更新

- [x] 6.1 修改 `src/routes/admin/clients.rs` `get_telemetry`，改为查询 `get_telemetry_summaries_by_device` 并以新 schema 返回

## 7. 前端：遥测 Worker 增强

- [x] 7.1 新建 `frontend/src/lib/device.ts`，实现 `collectDeviceFingerprint()` 返回设备静态信息
- [x] 7.2 在 `frontend/src/workers/telemetry.ts` 中将 `INTERVAL_MS` 改为 `5000`
- [x] 7.3 在 `startTelemetryWorker()` 中立即发送 `session_start` 事件（携带完整 device 对象）
- [x] 7.4 实现 behavior 增量追踪：click 监听器、scroll 深度、visibilitychange、路由变更
- [x] 7.5 实现 buffer swap 机制：发送前快照 behavior 计数器，失败时合并回当前 buffer

## 8. 前端：客户端锁定

- [x] 8.1 新建 `frontend/src/components/SystemLockedModal.tsx`，全屏遮罩 + 不可关闭弹窗
- [x] 8.2 在 `frontend/src/App.tsx` 中添加 `systemLocked` 信号，SSE `data_corrupted` 事件触发锁定
- [x] 8.3 在 App.tsx 根 JSX 中通过 Solid Portal 挂载 `SystemLockedModal`

## 9. 前端：管理后台遥测展示

- [x] 9.1 在 `frontend/src/api/admin.ts` 中将 `TelemetryRecord` 替换为 `TelemetrySummary` 接口
- [x] 9.2 修改 `frontend/src/pages/admin/ClientsPage.tsx` 遥测面板，渲染设备信息 / 会话统计 / 行为摘要 / 功能使用四个分类区块
