# WordForge v1.1.4 Backlog

> 起草日期：2026-06-03
> 来源：盘点工作流（6 线并行挖掘 → 归并去重 → 逐项回当前代码库对抗式核验，23 agent / 444 工具调用）
> 基线：分支 `feat/admin-ui` @ `v1.1.3-beta.2`，工作树已含 v1.1.3 全 19 项（W1–W5），最新迁移 `m044`。
> 颗粒度：每条 ≤ 2.5 人日（含测试与文档）；可直接拆为 GitHub issue。
> 字段：**维度** = arch-promise / perf-ops / honest-downgrade-followup / code-debt / client-coordination / feature；**优先级** = P1（重要债·契约/安全风险）/ P2（运维闭环·UX 收尾·代码债）；**估时** = 核验后修正值（单人日）。
> ⚠️ 每条「实施陷阱」由核验阶段回代码库逐行求证得出，是规划时点判断之外的真实修正，落地前务必读。

## 版本定位

v1.1.4 是 **v1.1.3 之后的收尾型 minor**，本质是把 v1.1.3 埋下的「基建已建、真正闭环未做」四块半成品（outbox / 告警收件箱 / 定时广播 / 离站备份）收口，并对 v1.1.x 期间累积的契约文档漂移止血。**全程无 P0**——生产硬校验本身正确生效、无线上断流，所有项是债务与闭环而非救火。

⚠️ **关键红线**：本版 **不翻** `RECORDS_OUTBOX_ASYNC` 默认值、**不删** `records/single.rs`+`batch.rs` 手动 rollback。那一步因异步响应不含 `amas_result` 仍需与学习端跨仓协同，v1.1.4 只打「精确一次 + 死信可运维」两块地基，为后续版本安全切换铺路。

> **✅ 红线已解除（2026-07-21 · v1.3.0-beta.1）**：三端学习客户端 ≥ 1.6.0 确认容忍无 `amasResult`
> 的 202 受理响应后，`RECORDS_OUTBOX_ASYNC` 默认已翻 `true`，路由层手动快照回滚已删除
>（失败恢复统一走 `processed_events` 幂等账本短路）。详见 `should-deferred.md` 后记 2 与
> CHANGELOG v1.3.0-beta.1。

## 总览

| 里程碑（执行波次） | 主题 | 任务数 | 估时 | 优先级分布 |
|---|---|---|---|---|
| **W1 S2 事件总线深化** | 兑现 R06 精确一次 + 死信可运维，为切默认 async 铺路 | 2 | 6.5 人日 | P1×2 |
| **W2 运营闭环补全** | D1 收件箱 / D2 定时广播 / B1 离站备份的「另一半」 | 4 | 5.5 人日 | P1×2 / P2×2 |
| **W3 性能运维加固** | 遥测背压 + 关停落盘 + 直方图精度 | 3 | 2.25 人日 | P1×1 / P2×2 |
| **W4 契约文档对齐 + 安全留痕** | v1.1.x OpenAPI/端点字典漂移止血 + 门控审计 | 6 | 2.75 人日 | P1×4 / P2×2 |
| **W5 代码债收口** | 死字段清理 | 1 | 0.3 人日 | P2×1 |
| **合计** | | **16** | **≈ 17.3 人日** | P1×9 / P2×7 |

---

## W1 · S2 事件总线深化（兑现 R06，为切默认 async 铺路）

> v1.1.3 的 S2-1 只落了 outbox 基建（默认 `RECORDS_OUTBOX_ASYNC=false` 走同步老路）。真正切默认 async 的两大拦路虎是「重启重复应用 AMAS」与「死信不可运维」。本波补齐这两块，**不动默认值**，故生产零暴露。

### W1-1 · 补 `processed_events` 幂等表，把「重启不丢」补成「精确一次」
- **维度**：arch-promise
- **优先级**：P1
- **估时**：4.0 人日
- **依赖**：已落地的 outbox 基建（m044）
- **描述**：当前 AMAS 状态持久化与记录落库**跨两个独立事务**：`process_single_record` 先 `state.amas().process_event`（AMAS 内部 `persist_engine_state_atomic` 自带 tx），随后另起 `run_store_task` 才在第二 tx 内 `update_elo` + `create_record_with_updates`。两 tx 之间崩溃后，outbox 重放会再跑 `process_one → process_single_record`，AMAS 状态被**二次累加（ELO/mastery/trust 全错）**。唯一去重 `check_duplicate` 只挡记录行重复插入，挡不住 AMAS 重复应用。新增 `processed_events` 幂等表（复用现成 `client_record_id` 作主键），把「未保证不重复」补成「精确一次」，为后续删手动 rollback + 切默认 async 铺第一块基石。single 与 batch 两入口同覆盖。
- **验收**：`m045` 建 `processed_events` 表；single/batch 入口在 AMAS 应用前查幂等表命中即短路；worker 重放命中幂等表直接 ack 删 outbox；集成测试覆盖「崩溃窗口重放不二次累加 ELO」；默认值 `records_outbox_async` 仍 false。
- **出处/证据**：`src/amas/engine.rs:328`（`persist_state` 在 `process_event` 内同步落）、`:1409-1411`（`persist_engine_state_atomic` 独立 tx）；`src/routes/records/single.rs:302-323`（先 AMAS）、`:327-419`（后另起 tx 落库）、`:447-452`（注释自承「保证不丢但未保证不重复」「单事务原子化属后续 cutover」）、`:255-261`（`client_record_id` 幂等键）；`src/routes/records/batch.rs:128-149`（同构去重）、`:169-179`（`prev_user_elo` 快照）；`src/workers/outbox_processor.rs:103-118`（重放走 `process_single_record`）；`src/config.rs:418,844`（默认 false）；`docs/v1-research/RFC.md:234`（R06）。
- **⚠️ 实施陷阱**：① **幂等查询必须前置到 `process_event` 之前**（在 `check_duplicate` 旁补 `processed_events` 查询）才能短路重复 AMAS；写 tx 内 `INSERT OR IGNORE` 只负责标记，放错位置会漏掉重复 AMAS——这是落地真正陷阱。② 把 AMAS 计算搬进记录同一 tx 代价过大（AMAS 内部已自开 tx + ELO 异步落），最小改法是用幂等标记让重放整体短路跳过 AMAS，而非真合并事务。③ batch 全失败回滚 `restore_user_state_snapshot`（`batch.rs:77-85`）与幂等表交互：重放时「部分已 processed、部分未 processed」的短路边界须与全失败回滚判定对齐，避免短路条被纳入回滚。

### W1-2 · outbox 死信运维闭环：明细列表 + 人工 requeue/丢弃 + 永久错误短路
- **维度**：arch-promise
- **优先级**：P1
- **估时**：2.5 人日
- **依赖**：无（可独立于 W1-1 先落地——重投后若记录已存在会自然短路）
- **描述**：`events_dead_letter` 表现只有「写入 + 计数」两个出口：store 层仅 `move_outbox_to_dead_letter`（写）+ `outbox_stats`（`SELECT COUNT`），无 `list/requeue/purge`；admin 只在 `system_health` 内嵌一个累计数，无任何明细/重投/丢弃端点；前端只有一个红色 chip 计数。一条记录重试 5 次进死信后永久躺表里，运维既不知是哪个 user 的哪条记录丢了 AMAS 处理，也无法人工重放。叠加 `process_one` 把 payload 反序列化失败 / 未知 event_type 与业务失败混为同一 `Err(String)`，**毒丸消息照样跑满 5 次退避（2^n 上限 300s）**才进死信。补：① store 加 `list_dead_letter` + `requeue_dead_letter`（镜像 `move_outbox_to_dead_letter` 反向单 tx）+ `purge_dead_letter`；② admin 加 `GET /admin/monitoring/dead-letter` + `POST .../dead-letter/:id/requeue` + 丢弃端点；③ `process_one` 区分 Permanent/Transient（仅 serde 失败 + 未知 event_type 判永久），永久错误跳过退避直接进死信；④ `MonitoringPage` 死信 chip 可点开抽屉列明细 + 重投/丢弃按钮。
- **验收**：死信可在 admin 列出明细（含 user/事件类型/失败原因/进死信时间）；requeue 后回 outbox 重新消费、attempts 归零；毒丸消息不再空跑 5 次退避；admin-ui 破坏性操作有二次确认 + e2e。
- **出处/证据**：`src/store/operations/outbox.rs:100-125`（`move_outbox_to_dead_letter` 原子 tx，requeue 可镜像）、`:128-153`（`outbox_stats` 仅 COUNT）、`:40-44`（`enqueue_outbox_event` attempts=0 可复用）；`src/routes/admin/monitoring.rs:30-41,85-93`（router 无死信端点，仅 `outbox_stats` 内嵌）；`src/workers/outbox_processor.rs:17`（MAX_ATTEMPTS=5）、`:64-84`（命中即 move + raise_data_alert 一去不回）、`:103-118`（错误混类）、`:20-23`（backoff）；`admin-ui/src/pages/MonitoringPage.tsx:386-397`（死信仅 chip）；`src/store/migrate.rs:2159-2167`（`events_dead_letter` 无唯一约束）。
- **⚠️ 实施陷阱**：① requeue 写回 outbox 必须 attempts 归零 + `next_retry_at=now`，且**在同一 tx 内 DELETE 死信行**（死信表无唯一约束，双投即双消费）；worker tick 与 requeue 端点并发同一 id 用单 tx 兜底。② 永久错误判定要**保守**：只有 serde 失败 + 未知 event_type 算永久，`process_single_record` 返回的 `AppError`（DB 锁 / AMAS 临时失败）必须仍按 transient 重试。③ 死信价值随 opt-in async 启用才兑现（默认同步老路死信恒空），但提前补齐是切默认 async 的运维前置。

---

## W2 · 运营闭环补全（D1/D2/B1 的「另一半」）

> v1.1.3 给 admin 落了告警收件箱、定时广播入队、离站备份上传，但三者都只做了一半：关键运营事件不进收件箱、排程不可撤销、备份成功无痕。本波补全闭环。

### W2-1 · canary 自动回滚事件接入 admin 告警收件箱（补 D1 覆盖盲区）
- **维度**：feature
- **优先级**：P1
- **估时**：0.5 人日
- **依赖**：D1 收件箱（已落地）
- **描述**：`canary_monitor` worker 每 5 分钟跑，命中 reward 降幅 / 异常升幅阈值会**自动回滚 patch canary**——这是最高优先级运营事件，却只 `set_patch_canary_status` + `tracing::warn!` + `broadcast_to_all_sse(Incident)`，**从不写 `system_alerts`**，因此永远进不了刚建好的收件箱。admin 关掉监控页就完全错过「我的 AMAS patch 被自动回滚了」。同库所有其它 worker 失败/事件（backup_offsite / scheduled_broadcast / daily_aggregation / metrics_flush）都已走 `record_system_alert` 入收件箱，canary 是唯一遗漏者。补一行 `store.record_system_alert("canary_monitor", kind, "warning", title, message)`。
- **验收**：自动回滚后收件箱出现未读告警（含被回滚 patch 的 `version_hash` + baseline/live reward & anomaly）；NotificationBell 未读角标 +1；SSE 与收件箱并存（不互替）。
- **出处/证据**：`src/workers/canary_monitor.rs:80-97`（仅 set_status + warn + SSE，无 record_system_alert）、`:43-44`（`run(state)` 已 `state.store().clone()`，可直接调）；对照 `src/workers/backup_offsite.rs:249`、`scheduled_broadcast.rs:112`、`daily_aggregation.rs:22/49/59`、`metrics_flush.rs:18/30`（均入收件箱）；`src/store/operations/system_alerts.rs:33-60`（`record_system_alert` 按 source+kind dedup）；`src/routes/admin/notifications.rs:35-50` + `admin-ui/src/components/layout/NotificationBell.tsx`（收件箱链路）。
- **⚠️ 实施陷阱**：① **dedup 真坑**：`system_alerts` `ON CONFLICT(source,kind)`。`get_active_patch_canaries` 返回多条，多个 patch 同周期回滚时若 kind 用静态 `"auto_rollback"` 会全部合并成一行 count++ **丢明细**；须把 `version_hash` 拼进 kind（如 `"auto_rollback:{version_hash}"`），message 不参与 dedup 可带详情。② **不需要 spawn_blocking**：该 worker 已在 async 上下文直调多个阻塞 SQLite 方法，新增 `record_system_alert` 直调即与既有惯例一致；勿照搬 `scheduled_broadcast.rs:111` 的 spawn_blocking（那是 interval loop 非 cron job）。③ severity 用 `warning`；别用 SSE 替代——Incident 瞬态刷新即丢，收件箱才持久可追溯。

### W2-2 · 定时广播队列 admin 查看/取消面板（闭合 D2 投递调度）
- **维度**：feature
- **优先级**：P1
- **估时**：2.5 人日
- **依赖**：D2 投递调度（已落地）
- **描述**：`m042/D2` 落地了「投递时机=指定时间」排程入队 + worker 每 60s 扫到期 fan-out，但**只做了入队半边**。store 层 `scheduled_broadcasts` 只有 `list_due`（worker 专用）+ `mark_sent/mark_failed`，没有列出全部 pending、按 id 取消、或编辑；broadcast 路由也只暴露 insert/draft，无 list/cancel scheduled；DevicesPage 排程提交后无队列视图。**误排一条错误内容或错误受众的未来广播，到点必然发出、无法撤销**。补：① `list_scheduled_broadcasts(status='pending')` + `cancel_scheduled_broadcast` 两个 store op + GET/DELETE 路由；② DevicesPage「待发排程」列表（标题/受众/计划时间/取消按钮）。
- **验收**：admin 可看待发排程列表并取消；取消后 worker 不再 fan-out；DevicesPage 显示队列。
- **出处/证据**：`src/store/operations/scheduled_broadcasts.rs:84-141`（仅 list_due/mark_sent/mark_failed，无 list_pending/cancel）；`src/routes/admin/broadcast.rs:15-28,252-295`（router 无 scheduled list/cancel，仅入队后立即返回）；`src/workers/scheduled_broadcast.rs:38-130`（单向消费者）；`admin-ui/src/pages/DevicesPage.tsx:652-714`（排程提交即清表单无队列视图）；`admin-ui/src/api/admin.ts:230-269`（无 listScheduled/cancelScheduled）；`src/store/migrate.rs:2295`（CHECK 约束）。
- **⚠️ 实施陷阱**：① **会直接炸的关键点**：`scheduled_broadcasts` 的 CHECK 约束 = `status IN ('pending','sent','failed')`，**不含 `'canceled'`**，直接 `UPDATE SET status='canceled'` 触发 SQLite CHECK 违反。SQLite 无法 ALTER 约束，必须新增 `m045` 重建表（CREATE new + INSERT SELECT + DROP + RENAME）把 `'canceled'` 加入枚举。② cancel 用 `UPDATE...WHERE id=? AND status='pending'` 原子抢占（受影响行数=0 即已被 worker fan-out，返回 409 语义），防与 60s 扫描竞态把已发出的误标取消。③ `scheduled_at` 是 RFC3339 字符串列，取消不改格式；受众四维列表回显沿用 `decode_str_list` 的 NULL=不过滤语义；`idx_scheduled_broadcasts_due(status,scheduled_at)` 已可服务 list_pending 无需新索引。

### W2-3 · admin 告警收件箱补「全部已读」批量操作
- **维度**：feature
- **优先级**：P2
- **估时**：0.5 人日
- **依赖**：D1 收件箱（已落地）
- **描述**：D1 收件箱后端只有 `POST /:id/read` 单条标记，没有 mark-all-read；store 层只有 `mark_system_alert_read` 单条；NotificationBell 只能逐条点。worker 周期性失败会积累几十条未读，清空角标需逐条点。end-user 那套 notifications 反而有 read-all，FeedbackPage 也有 `markAllFeedbackRead` 成熟范式。补：`mark_all_system_alerts_read` + `POST /api/admin/notifications/read-all` + NotificationBell 面板头「全部已读」按钮。
- **验收**：一键清空未读角标；返回重算后的 unreadCount。
- **出处/证据**：`src/routes/admin/notifications.rs:19-23`（无 read-all）；`src/store/operations/system_alerts.rs:121-132`（仅单条）、`:54-56`（同源同类再发会重置 read_at）；`admin-ui/src/components/layout/NotificationBell.tsx:56-69`（逐条）；对照 `src/routes/notifications.rs:21`（end-user 有 read-all）+ `admin-ui/src/pages/FeedbackPage.tsx`（`markAllFeedbackRead`）。
- **⚠️ 实施陷阱**：① 批量版保持幂等不覆盖首次时间：`UPDATE...SET read_at=COALESCE(read_at,now) WHERE read_at IS NULL`，别全表覆盖已有 `read_at`。② `record_system_alert` 在同源同类再次发生时会把 `read_at` 重置 NULL（设计如此），所以 mark-all **不是永久清空**，UI 文案别误导成「永久已读」。③ `ackedBy` 当前与 read 耦合，分离 ack 语义牵动 `m041` schema 超出 minor 范围，本项只做 read-all。

### W2-4 · 离站备份（B1）执行状态可观测 + 单 target 连通性测试
- **维度**：feature
- **优先级**：P2
- **估时**：2.0 人日
- **依赖**：B1 离站备份（已落地）
- **描述**：v1.1.3 B1 落地了每日本地备份后按 `BackupTarget.uri`（file/rsync/s3）推送离站 + 失败告警，但**只有「失败」走 `record_system_alert`，「成功」无任何痕迹**：admin 在 settings 配了 `s3://` target，却无从知道离站备份昨天是否真传上去、上次成功时间、各 target 状态。每日备份 loop（`main.rs` 独立 interval，非 WorkerManager）也没调心跳，连 worker 列表都看不到它。补：① backup_offsite 成功后落 `upsert_worker_last_run` 心跳；② 每 target `last_ok_at/bytes/失败原因` + settings BackupRenderer 状态列；③（stretch）「测试连通」按钮对 s3/rsync 做 dry-run 探测。
- **验收**：admin worker 列表 + Prometheus gauge 出现 `backup_offsite` 心跳；每 target 显示上次成功/失败；灾备从「配了不知有没有用」变为可验证。
- **出处/证据**：`src/workers/backup_offsite.rs:56-58`（成功仅 tracing::info）、`:248-250`（仅失败 record_system_alert）、`:205`（凭据注入路径）；`src/main.rs:340-362`（独立 interval loop 零心跳，注释明写「与 cron worker 解耦」）；`admin-ui/src/pages/settings/SectionRenderers.tsx:380-419`（BackupRenderer 无状态列）；`src/routes/admin/settings_sections.rs:525-532`（BackupTarget 仅 name/uri/retention_days）；现成基建 `src/store/operations/worker_last_run.rs:18`（`upsert_worker_last_run`，自动喂 admin + Prometheus）。
- **⚠️ 实施陷阱**：① 心跳用 `upsert_worker_last_run`（非候选笔误的 `record_worker_run`），且**接入要落在 `backup_offsite::run` 内部**——`main.rs:346` 那个 loop 仅在有 `backups_dir` 时 spawn，落在内部才保证覆盖真实分支。② part② 不要塞进 settings backup-policy section（那是用户可编辑配置，混入运行时状态会与 PATCH 覆写冲突），新建轻量 `backup_target_status` 状态行。③ 连通测试成本不对称（s3 建 client 做 head/list、rsync spawn dry-run，凭据路径须与正式上传同源勿另起 env），作为 stretch 拆出。④ 与诚实降级第 1 条（探针 sink 裁剪）无关，勿混淆。

---

## W3 · 性能运维加固

### W3-1 · telemetry 专用 per-user 限频 + 端点级 backpressure 守卫
- **维度**：perf-ops
- **优先级**：P1
- **估时**：1.5 人日
- **依赖**：N1 pool 16→8（已落地，本项是其背压收紧的逻辑延续）
- **描述**：为 `/api/telemetry` 增加一条独立于通用 API 限流的、更紧的 per-user 遥测频率配额（env 可配）。遥测当前与全部业务 API 共吃同一条 authenticated 预算，单个噪声客户端可在该预算内把整条 SQLite 写路径（经全局 blocking 信号量）打满，**挤占学习数据写入**——pool 已收到 8，写路径是最稀缺资源。payload size guard 已有（`DefaultBodyLimit` 64KB），本项补 per-user 限频 + 超额软丢弃（`received:true, throttled:true` 不落库，复用现有 `sampledOut` 早返回模式）。属 R32 限频子项，零迁移。
- **验收**：单客户端超配额后遥测被软丢弃但设备活跃度仍刷新；学习数据写入不受噪声遥测拖累；env `RATE_LIMIT_TELEMETRY_MAX` 可调。
- **出处/证据**：`src/routes/telemetry.rs:13-21`（仅 DefaultBodyLimit，无 per-user 层）；`src/routes/mod.rs:83,116,122-125`（telemetry 已被通用 `rate_limit_middleware` 覆盖）；`src/middleware/rate_limit.rs:91-148`（`check_with_max` 支持任意 key + 显式 max，可复用）、`:231-237`（telemetry 走 authenticated 600/window）；`src/config.rs:349-354`（默认 window 900s，无 telemetry 专项）；`src/state.rs:174-175,250-266`（仅 rate_limit + auth_rate_limit 两个并列 limiter）；`src/blocking.rs:39-57`（全局信号量按 pool=8 初始化，无端点级隔离）。
- **⚠️ 实施陷阱**：① 遥测**已被通用限流覆盖（非零保护）**，别误判为做整层中间件——只需 handler 内追加一次更紧的 `check_with_max`。② 复用全局 `RateLimitState` 会与业务流量串味，应**新建独立 telemetry limiter 实例**（参照 `auth_rate_limit` 并列模式 + 独立 cleanup loop），否则 cleanup 周期与配额语义互相污染。③ 配额单位须**显式定义 window**——现状是 600/15min 不是 600/min，文案勿照抄「每分钟」。④ throttled 早返回点必须放在 `upsert_client_device` 之后、与 `sampledOut` 同位（`telemetry.rs:319`），否则绕过 `last_seen` 刷新误伤在线判定。

### W3-2 · `availability_rollup` 补关停最终落盘，消除重启丢失登录 SLO 当前小时桶
- **维度**：code-debt
- **优先级**：P2
- **估时**：0.25 人日
- **依赖**：m039 SLO 持久化（已落地）
- **描述**：登录页 SLO 30d 持久化靠 `metrics_flush` worker 每 5 分钟 flush 内存 hour 桶到 `availability_rollup`。graceful shutdown（含一键自更新重启）时**没有触发最终 flush**，导致最近一次 flush 到关停之间（≤5 分钟）的请求/5xx 计数随内存丢失。在 `server_future.await` 返回后、main 退出前补一次 `flush_availability_rollup`。
- **验收**：关停前最后 ≤5min 增量落盘；重启后 SLO 桶无缺口。
- **出处/证据**：`src/workers/metrics_flush.rs:43`（`flush_availability_rollup` 仅 5min cron 调用）；`src/workers/mod.rs:251`（cron `0 */5 * * * *`）；`src/main.rs:647-665`（`shutdown_signal` 仅 `shutdown_tx.send`，无 flush）、`:431,445`（with_graceful_shutdown + server_future.await）、`:124-140`（启动回灌已有）；`src/store/operations/availability.rs:26-31`（整桶覆盖式 upsert，幂等）。
- **⚠️ 实施陷阱**：① **候选 evidence 有一处事实错误已纠正**：`export_hour_rollup`（`http_metrics.rs:289-299`）是**只读 snapshot 非 drain**，不清内存——故「shutdown flush 与 cron tick 竞态重复 flush」前提不存在，且整桶覆盖式 last-write-wins 无害。② 影响被高估：SLO 读 `availability_pct` 比率，分子分母按比例同损大致抵消，实际仅个别 hour 桶绝对计数亚千分级扰动，故 P2 + 0.25d。③ `flush_availability_rollup` 现为 `pub(crate)` 私有 fn，main.rs 直调需提升可见性或抽公共入口。④ final flush 须放 `server_future.await` 之后（此刻不再有新请求写内存桶）。⑤ AMAS 指标 flush（`snapshot_and_reset` 真 drain）有同样关停丢失行为，本条**只碰 availability 旁路**，别顺手算进估时。

### W3-3 · `http_request_duration_seconds` histogram 细化中段 bucket 边界
- **维度**：perf-ops
- **优先级**：P2
- **估时**：0.5 人日
- **依赖**：R31 主体 `/metrics` 端点（已落地）
- **描述**：把 `BUCKET_BOUNDS` 从 `[0.01,0.05,0.1,0.5,2.0]` 细化为如 `[0.01,0.025,0.05,0.1,0.25,0.5,1.0,2.5]`，补齐 0.1→0.5→2.0 之间的塌陷区间。当前 100ms~2s 只有 2 个 bucket，`histogram_quantile` 对 p95/p99 的线性插值在这段严重失真——而慢请求恰是 SLO 关注点。纯常量 + 测试 + 迁移守卫。
- **验收**：`/metrics` 输出新桶边界；p95/p99 精度提升；历史 series 断层在 runbook 注明。
- **出处/证据**：`src/middleware/http_metrics.rs:23`（`BUCKET_BOUNDS`）、`:39/164/223/308/423/473`（均按 `len()+1` 派生，改常量自动跟随）、`:304-334`（`import_hour_rollup` 对旧向量 `buckets.resize`）；`src/routes/metrics.rs:45-189`（exposition）；`src/main.rs:124-140`（启动回灌调 import）。
- **⚠️ 实施陷阱**：① **候选 trap（第二处 buckets 字段 panic）是杜撰**，已核实 `BUCKET_BOUNDS` 是唯一真值源、无第二维度。② **真陷阱在 D3 持久化**：`availability_rollup` 把 hour 桶 JSON 持久化，启动 `import_hour_rollup` 对旧向量执行 `buckets.resize(new_len, 0)`——桶数从 6 增到 9 时旧 `+Inf` 计数滞留 index 5、新 `+Inf` 落 index 8 读到 0，**破坏历史小时桶累积直方图、污染登录页 30d SLO 的 p99**。须给 `availability_rollup` 加桶版本标记，或首次升级 import 时丢弃/重置 pre-upgrade 行（与现有 effective_secs 如实标注窗口的不变式一致，不伪造历史）。③ runbook 注明发版窗口与 Prometheus 端历史 series 断层。

---

## W4 · 契约文档对齐 + 安全留痕（v1.1.x 漂移收尾）

> v1.1.3 加了 ~90 端点但 OpenAPI 规格、端点字典、门控审计多处滞后于实现。本波纯文档 + 少量低风险代码，可快速转正。

### W4-1 · `openapi.yaml` `info.version` 从 0.6.0-beta.4 同步到当前版本
- **维度**：client-coordination
- **优先级**：P1
- **估时**：0.5 人日
- **依赖**：无
- **描述**：`docs/openapi.yaml` 由 `src/openapi.rs` 经 `cargo test --test openapi_export` 自动生成 + CI diff 守卫，但 `info.version` 硬编码为 `0.6.0-beta.4`——**跨整条 v1.1.x 从未更新**，当前二进制是 `v1.1.3-beta.2`，OpenAPI 规格对外仍自称 `0.6.0-beta.4`，误导所有以 `openapi.yaml` 为契约源的客户端/工具链。改为从 `env!("CARGO_PKG_VERSION")` 取值。
- **验收**：`docs/openapi.yaml` version 与 `Cargo.toml` 一致；`cargo test --test openapi_export` 通过；CI diff 绿。
- **出处/证据**：`src/openapi.rs:978`（`.version("0.6.0-beta.4")` 硬编码）；`docs/openapi.yaml:9`（陈旧产物）；`tests/openapi_export.rs:9`（导出）、`:23`（字面断言）；`.github/workflows/openapi-drift.yml:51,55`（守卫）；对照 `src/routes/health.rs:156`（已用 `CARGO_PKG_VERSION` 范式）。
- **⚠️ 实施陷阱**：① 取值源用 `CARGO_PKG_VERSION` **而非 GIT_VERSION**——后者经 `build.rs git describe` 派生，CI shallow checkout 无 tag 会 fallback，导致本地与 CI 导出 version 行不同把 drift 守卫搞 flaky。`CARGO_PKG_VERSION` 确定性、无 v 前缀。② **硬 blocker**：`tests/openapi_export.rs:23` 有字面断言 `contains("0.6.0-beta.4")`，不同步改测试会**先 panic、文件根本写不出去、CI 永远红**——候选完全没提这条。③ 勿误伤：`openapi.rs:473/511/880` 与 `middleware/deprecation.rs:62` 的 `v0.6.0-beta.4` 是「自某版本起」的 since-version 文档，与 `info.version` 无关，绝不能一并替换。

### W4-2 · `api-endpoints.md §15` 遥测段补齐 m038 四要素硬校验 + 三态归属 403
- **维度**：client-coordination
- **优先级**：P1
- **估时**：0.5 人日
- **依赖**：无
- **描述**：`api-endpoints.md §15` 遥测段是面向客户端开发者的权威端点字典，但完全是 m038 前的旧文档：必填头只列 `x-device-id`，没提 `x-device-platform`/`x-app-version` 必填；示例 `payload.device` 缺 `model`；字段表无 `device.model`/header 必填，也无两个 403。**任何客户端按此文档原样上报，在生产（已 beta.4 硬校验生效）必被拦**。补四要素必填表 + 三态归属表 + 示例加 model，与 `api-spec.md §11`、`v1-client-migration.md §5.1` 对齐。纯 md。
- **验收**：四要素必填头 + payload model + 两 403 三态表齐全；与 api-spec/migration 三处一致。
- **出处/证据**：`docs/api-endpoints.md:2389-2472`（遥测段无 m038）、`:2401`（仅 x-device-id）、`:2410-2423`（device 缺 model）、`:2446-2453`（字段表缺）；`src/routes/telemetry.rs:134/142/153/164`（四要素硬校验）、`:216/224`（两 403）；对照已对齐 `docs/api-spec.md:296-324`、`docs/v1-client-migration.md:237-284`。
- **⚠️ 实施陷阱**：① W1（v1.1.3）只对齐了 api-spec/v1-client-migration，**这份最常被引用的端点字典被漏掉**——别误以为遥测契约文档已全量对齐。② 缺口比「补 model」更宽，连两个必填 header 都没文档化，客户端会先撞 `MISSING_OS`/`MISSING_APP_VERSION`，四要素都要补。③ 占位值口径照 `api-spec.md §11.2`（「`browser on OS` 派生标识或 `web-admin` 占位」），勿臆造新约定。④ 优先级由候选的 P0 下调 P1：生产硬校验本身正确生效、无运行时断流，止血主路径已由 api-spec/migration 覆盖，这是契约文档准确性问题。

### W4-3 · `DELETE /api/users/me` 与 `GET /api/users/me/export`（GDPR）补入 `api-endpoints.md`
- **维度**：client-coordination
- **优先级**：P1
- **估时**：0.5 人日
- **依赖**：无
- **描述**：GDPR 注销（`DELETE /api/users/me` 级联清用户表）与数据导出（`GET /api/users/me/export`，NDJSON 流式 + 24h 冷却 + 429）两端点已实现稳定，但 `api-endpoints.md` 用户段只列了 GET/PUT/PUT password/GET stats，完全没有这两条。**合规端点未进权威端点文档 = 对外不可发现**。补两节：DELETE 说明级联不可逆；export 说明 NDJSON `{table,data}` 行格式、24h 冷却、429 + Retry-After。
- **验收**：用户段含 DELETE 与 export 两节；契约措辞精确。
- **出处/证据**：`src/routes/users.rs:31`（`.delete(delete_me)`）、`:35`（`/me/export → gdpr_export`）、`:101-110`（`delete_user`）、`:217-293`（NDJSON 契约）、`:240-253`（429 三字段）、`:22`（冷却常量）；`docs/api-endpoints.md:262-335`（用户段缺这两条）；`src/store/operations/users.rs:12-39`（`USER_SCOPED_TABLES` 26 张）。
- **⚠️ 实施陷阱**：① export/delete **范围不对称**：export 仅 7 张可移植表（profile/study_config/word_states/favorites/notes/sessions/records，Article 20 可移植性），delete 级联清 26 张表 + 用户自建 wordbooks——文档别把 export 写成「全量导出」否则成新合规误述。② 429 body 故意只有 `{success,code,message}` 三字段，照抄勿补。③ TOC 只列模块级章节（用户模块锚点已存在），纯补两节正文即可，无需新增锚点。

### W4-4 · `min_client_version` 版本门控变更补 admin 审计留痕
- **维度**：code-debt
- **优先级**：P1
- **估时**：0.5 人日
- **依赖**：D4 版本门控（已落地）
- **描述**：D4 客户端最低版本门控是**「一键锁死全体客户端」的高危生产控制**——开启后 strict-mode 按版本门挡掉所有低版本客户端。但 `set_version_gate` 改这两字段时只发 `tracing::info!`，**没有写 `update_audit_log` 审计表**。同库 `resource_packs` 激活/下架等同级变更都有审计留痕。这种开关却查不到谁、何时、从什么版本改到什么版本。在 `set_version_gate` / `update_settings` 两条入口调 `insert_admin_audit`（action=`set_version_gate`，记 old→new + enabled 翻转）。
- **验收**：两条入口改门控后 `update_audit_log` 落行（含 old/new min_client_version + enabled）；集成测试断言落行。
- **出处/证据**：`src/routes/admin/settings.rs:328-376`（`set_version_gate` 仅 tracing::info）、`:177-178,219-229,246-251`（`update_settings` 版本门控分支同样仅日志）；`src/store/operations/update_audit.rs:106-136`（`insert_admin_audit`，SQL 把 from/to/channel 硬编码空串）；对照 `src/routes/admin/resource_packs.rs:232/289/349`（均写审计）；`src/state.rs:308-317`（effective 值 `.or_else()` 回落 env）。
- **⚠️ 实施陷阱**：① 候选说「调既有 `write_admin_audit`」不准——那是 `resource_packs.rs:471` 文件内私有 fn 且硬编码 `target_type="resource_pack"`，不可跨模块复用；settings 应直接调 `state.store().insert_admin_audit(&admin_id, "set_version_gate", Some("settings"), Some("version_gate"), Some(&metadata))`，容错（Err 仅 warn 不阻塞）。② 必须在进 `run_store_task` 闭包前先取旧 `min_client_version` 记 old 值（闭包 move 消费 req、内部是 get 后改）。③ 审计记 settings 层原始值（含 None）**而非 effective 值**——effective 会把 env 兜底误记成人为改动。④ from/to/channel 已由 SQL 层硬编码空串，调用方不传，勿照搬资源包语义。

### W4-5 · check-update 端点弃用决议落地（释疑 release-calendar「v1.1 删除」）
- **维度**：client-coordination
- **优先级**：P2
- **估时**：0.5 人日
- **依赖**：无
- **描述**：`release-calendar.md:75` 与 `RFC.md:357` 仍承诺 `GET /api/admin/monitoring/check-update`「v1.1 删除」，但当前已是 v1.1.3，端点不仅未删，反而是 admin-ui 多处活跃依赖（Dashboard 顶栏更新角标、Monitoring 版本卡）的**轻量只读版本探测**，与重型 `/admin/updates/*` 自更新机定位不同、不可无损互替。**承诺与实现持续背离**。本仓可交付：路径 A（推荐）保留为内部 admin 端点 → 改两份文档措辞 + 可选用现成 `make_deprecation_layer` 注 Deprecation/Sunset header；路径 B → v1.2 删除并迁 Dashboard 到 `updatesStatus().stable?.hasUpdate`。
- **验收**：文档承诺与实现一致；若保留则注弃用 header；不再有长期失信条款。
- **出处/证据**：`docs/release-calendar.md:75` + `docs/v1-research/RFC.md:357`（承诺 v1.1 删除）；`src/routes/admin/monitoring.rs:34,152-188`（端点完整运行 + TTL 缓存，未注 deprecation）；`src/middleware/deprecation.rs:29`（`make_deprecation_layer` 基建，仅 `v1.rs:29-32` 套用）；`admin-ui/src/api/admin.ts:169`（checkUpdate）；`admin-ui/src/pages/DashboardPage.tsx:60`（createResource 无 catch，但 `:703/904` 用可选链）；`MonitoringPage.tsx:102`（`Promise.allSettled` 对 404 有韧性）。
- **⚠️ 实施陷阱**：① 候选称「删端点破坏 admin-ui 多处 UI」**程度被高估**——实测 Dashboard 全程用 `updateInfo()?.` 可选链，errored resource 只「静默丢版本角标」非整页崩；Monitoring 走 allSettled 完全无感，是角标失数级软退化。② 但 check-update（TTL 缓存只读探测）与 `/admin/updates/*`（重型 apply/rollback/backup）语义确不可无损互替，**强删迁移收益小于风险**，建议取路径 A。③ 若选保留，`release-calendar:75` + `RFC.md:357` 两条「v1.1 删除」必须撤销否则成长期失信。

### W4-6 · 跨仓 device.model 协同收尾：补 403 客户端降级处置矩阵 + 登记排期
- **维度**：client-coordination
- **优先级**：P2
- **估时**：0.25 人日
- **依赖**：外部仓 wordforge-web 节奏（不阻断本仓发版）
- **描述**：`release-calendar.md` 已登记 v1.1.3 T1 待协同项（wordforge-web 补 `device.model` + 约定最低后端版本），状态停在「待维护者排期」。`v1-client-migration.md` 与 `api-spec.md` 已有三态后端行为表与合并式降级指引，**真正缺的只是「按两个 403 码区分的客户端 UX 处置矩阵」**：`DEVICE_NOT_REGISTERED`（设备未注册，可引导正常登录使用后由首登用户 claim 自动恢复，可保留遥测队列）vs `DEVICE_OWNERSHIP_MISMATCH`（归属冲突，会持续 403、须静默丢弃不重试）。补这张矩阵并挂到 release-calendar T1 段作跨仓联调验收清单。纯文档。
- **验收**：现有三态表旁补 403 码差异化处置矩阵；release-calendar T1 段挂接验收清单。
- **出处/证据**：`docs/release-calendar.md:83-85`（wordforge-web 表 `_待填写_`）、`:89-100`（T1 协同项停在待排期）；`src/routes/telemetry.rs:212-231`（两 403 语义 + `Some(None)` claim 放行，m038 故意硬拦截）；`docs/v1-client-migration.md:257`（已有合并式降级指引）；`docs/api-spec.md:319-324`（三态表）。
- **⚠️ 实施陷阱**：① 两个 403 处置语义**不同别混为一谈**：NOT_REGISTERED 可自动 claim 恢复、OWNERSHIP_MISMATCH 永久 403 须丢弃。② 清单**严禁承诺放宽后端行为**——这两 403 是 m038 故意硬拦截无灰度开关，客户端只能降级不能要求后端放宽。③ 现状非「完全无降级建议」（80% 已在文档），故估时收窄至 0.25d；本仓侧文档可独立交付，无需等 wordforge-web。

---

## W5 · 代码债收口

### W5-1 · 清理死字段：admin-ui `api/health.ts` 整模块零引用 + 后端 `consecutiveFailures` 占位
- **维度**：honest-downgrade-followup
- **优先级**：P2
- **估时**：0.3 人日
- **依赖**：无
- **描述**：`admin-ui/src/api/health.ts` **整模块**（`healthApi` + 3 个 interface）在全站零外部导入（真实在用的库监控走 `api/admin.ts getDatabase()`）。后端 `health.rs` `database_health()` 仍硬编码 `consecutiveFailures: if healthy {0} else {1}` 占位 + TODO，但其前端消费方是死代码故对运维完全不可见。删 admin-ui 死模块；后端字段保守处理（先改 TODO 注释明确「0/1 为单次探活布尔非真实连续失败计数」，待跨仓确认无消费方后删字段）。
- **验收**：admin-ui 死模块删除、lint/build 通过无回归；后端字段决策落地。
- **出处/证据**：`admin-ui/src/api/health.ts:1-47`（整文件零外部 importer，Grep 已证）；`src/routes/health.rs:97`（`/database` 公开挂载）、`:209-214`（占位 + `:212` TODO）；`admin-ui/src/api/admin.ts:168`（真实在用的 `getDatabase→DatabaseInfo` 不含该字段）。
- **⚠️ 实施陷阱**：① 死模块是**整文件 1-47**（`healthApi` 与所有方法一并死），非候选所写的局部。② admin-ui 死模块可**无条件删**（纯本仓零引用）；后端字段删前必须**跨仓确认** wordforge-web/监控脚本未直接读该 JSON 字段（`/health/database` 公开可达，本仓内无法求证），故保守先改注释。③ `store_probe_ok` 是单次布尔，真实现连续计数需 AppState 加共享计数器，价值与成本不匹配——**应删字段而非补实现**。

---

## 明确不做（划界，核验阶段剔除，勿误捡进 issue）

### 已核验 = 应继续搁置（4 项）
1. **probe REPL `ctx.idb.count` 写死 -1**：设备端 IDB count IO 重、顶破 `IDB_LIST_TIMEOUT_MS` 200ms cap，消费面仅 REPL 专家，降级理由 v1.1.4 仍成立。
2. **数据探针事件流 5s 轮询 → 真实 SSE**：SQLite 无原生 pub/sub，真 SSE 须在遥测写入热路径挂 broadcast fan-out 污染性能敏感路径，或后端轮询 goodput 不升反降——过度工程，触碰诚实降级第 1 条延伸。
3. **AMAS 三个 disabled 预设落地 + 「另存为预设」**：conservative 预设涉及关闭 IAD/MTP/SSP，撞诚实降级第 2 条；卡片 explore_eps/cooldown 是纯展示占位（schema 无对应覆盖映射），落地前提不成立——属 v2 级产品决策非债。
4. **设备推送 composer 优先级控件**：真实价值锁在跨仓客户端是否按优先级差异化展示，本仓无客户端代码，与诚实降级第 7 条（APNs/FCM 推送协议层未就绪）同属未验证前提的跨仓 v2 评估项。

### 仍属 v2 / 范围不匹配
- 多实例 / leader 选举 / 集群升级 / HA（需服务发现）
- 灰度百分比发布（需多实例 + 服务发现，D4 已收窄为版本门控）
- admin GUI 全量 i18n（抽 118 个 .tsx 硬编码中文 + 引框架，运维面无产品收益）
- 协作 / 班级 / 订阅 / 付费墙 / OAuth / 切 PostgreSQL / per-user 算法 fine-tune

### ⚠️ 7 项刻意诚实降级，规划者勿误当 backlog 回退（均带代码注释标记为有意保留）
1. 探针 sink 永久裁剪为真实 SQLite 表 + 派生探针（无 ClickHouse/Kafka/S3）
2. AMAS 甜甜圈 6 个路由算法是正确口径（设计稿「8」是语义错误，IAD/MTP/SSP 是记忆模型不参与路由）
3. resource-packs 三态 `installed/verify_failed/rollback` 对齐真实枚举
4. feedback 无投票数据源故不臆造票数
5. broadcast 真实 in-flight 状态非伪造逐批 SSE
6. analytics 无离开用户决策序列端点故不伪造
7. APNs/FCM 多渠道需外部推送厂商，继续保持 disabled

**这些不是待办，不应回退。**

---

## 附：优先级速查

| 优先级 | 任务 | 估时 |
|---|---|---|
| **P1** | W1-1 幂等表 4d / W1-2 死信运维 2.5d / W2-1 canary 进收件箱 0.5d / W2-2 定时广播队列 2.5d / W3-1 遥测限频 1.5d / W4-1 openapi 版本 0.5d / W4-2 端点字典补 m038 0.5d / W4-3 补 GDPR 端点 0.5d / W4-4 门控审计 0.5d | 13.0d |
| **P2** | W2-3 全部已读 0.5d / W2-4 离站备份可观测 2d / W3-2 关停落盘 0.25d / W3-3 直方图细化 0.5d / W4-5 check-update 决议 0.5d / W4-6 403 降级矩阵 0.25d / W5-1 清死字段 0.3d | 4.3d |

## 附：交叉引用速查

| 来源锚点 | 对应 v1.1.4 任务 |
|---|---|
| RFC R06 / S2-1（v1.1.3 仅基建） | W1-1, W1-2 |
| RFC R32（telemetry 背压） | W3-1 |
| RFC R31（metrics 桶精度） | W3-3 |
| v1.1.3 D1 收件箱覆盖盲区 | W2-1, W2-3 |
| v1.1.3 D2 定时广播半实现 | W2-2 |
| v1.1.3 B1 离站备份可观测缺口 | W2-4 |
| v1.1.3 D4 门控无审计 | W4-4 |
| W1 遥测契约文档漏 api-endpoints.md | W4-2 |
| GDPR 端点未进字典 | W4-3 |
| RFC §10 / release-calendar check-update 删除承诺 | W4-5 |
| v1.1.3 T1 ③ 跨仓协同 | W4-6 |
| S7 残留死字段 | W5-1 |
