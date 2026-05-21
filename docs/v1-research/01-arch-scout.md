# 01 · 代码 / 架构现状盘点（arch-scout）

> 调研日期：2026-05-21
> 仓库版本：Cargo.toml `0.6.0-beta.4`（main HEAD `d0325f8`，最近发版 tag `v0.6.0-beta.3`）
> 范围：`src/`（46072 行 Rust）+ `frontend/src/`（14849 行 TS）+ `tests/`（35 个集成测试，13489 行）+ `frontend/tests`（116 个 vitest）+ `e2e/`（11 个 playwright spec）

---

## 1. 模块清单（按职责分组）

### 1.1 后端 Rust（`src/`）

| 组 | 文件 / 子目录 | 行数 | 职责 |
|---|---|---|---|
| **入口** | `main.rs` | 480 | bootstrap：Config / Store / AMAS / state / router / workers / TLS / self-watchdog |
| **入口** | `lib.rs` | — | crate re-exports |
| **配置** | `config.rs` | 530+ | `Config` + `StrictModeConfig` + LLM / rate-limit / update-check / worker 等 12 个子配置；从 env 装载，启动时 fail-fast 校验 |
| **存储** | `store/mod.rs` | 200+ | rusqlite + r2d2 单池：`PRAGMA journal_mode=WAL, synchronous=NORMAL, busy_timeout=N, cache_size=-64000, mmap_size=256MiB, temp_store=MEMORY` |
| **存储** | `store/schema.rs` | 637 | DDL（97 个 CREATE TABLE/INDEX，全量 schema） |
| **存储** | `store/migrate.rs` | 540+ | 14 条编号 migration（m001-m014），跳号 m013/m014 顺序由 `migrations()` 数组保证 |
| **存储** | `store/keys.rs` | — | sled-时代的 key 拼装工具，仍被 sled→sqlite 迁移代码引用 |
| **存储** | `store/operations/*.rs` | 11648 | 25 个表级 CRUD 模块（最大 `extras.rs` 1134 行 / `learning_sessions.rs` 947 / `records.rs` 903 / `admin_analytics.rs` 889） |
| **AMAS 引擎** | `amas/engine.rs` | 1900+ | `AMASEngine`：`process_event` / `get_user_state_async` / `compute_strategy_from_state` / `reload_config`；持锁顺序 user_lock → config.read |
| **AMAS 决策** | `amas/decision/{heuristic,ige,swd,ensemble}.rs` | 891 | 3 决策器 + 信任分加权融合 |
| **AMAS 记忆** | `amas/memory/{mdm,mastery,iad,mtp,ssp,evm,benchmark_adapter}.rs` | 2188 | 多模型记忆曲线（MDM 主）+ SSP 预计算表 |
| **AMAS 配置** | `amas/config/*.rs` | — | 16 个子配置块 + `tuning_whitelist.rs`（LLM 调参安全区间） |
| **AMAS 杂项** | `amas/word_selector.rs` | 711 | new vs review 池调度 + UCB |
| **AMAS 杂项** | `amas/{elo,monitoring,metrics,metrics_persistence,types,constants}.rs` | — | ELO / monitoring / metrics 持久化 |
| **路由** | `routes/mod.rs` | 159 | router 装配 + 4 个 middleware layer 顺序 |
| **路由（用户）** | `routes/{auth,users,words,records,learning,study_config,user_profile,word_states,word_favorites,word_notes,wordbooks,wordbook_center,notifications,content,realtime,feedback,telemetry,health,status,probe_results,analytics}.rs` | 6900+ | 用户 + 公共 |
| **路由（V1）** | `routes/v1.rs` | 209 | **绕过 AMAS** 的兼容层，详见 §3.1 |
| **路由（admin）** | `routes/admin/{auth,mod,amas,analytics,broadcast,clients,feedback,monitoring,probe,settings,updates}.rs` | 3870 | 11 个 admin 子路由（其中 `admin/amas.rs` 988 行是 v0.6 重头） |
| **服务层** | `services/admin.rs` | 225 | list_users / ban_user / stats / set_user_password |
| **服务层** | `services/learning.rs` | 25 | **只是 Store + AMAS 两个 Arc 的容器，无业务方法** |
| **服务层** | `services/wordbook.rs` | 79 | 词书中心 URL 读写 |
| **服务层** | `services/probe.rs` | 340 | 远程探针：dispatch / confirm / result（含 SSE 桥） |
| **服务层** | `services/updater.rs` | 1217 | admin 自更新（list_releases / channels / apply tar / progress SSE） |
| **服务层** | `services/llm_provider.rs` | 266 | OpenAI 兼容 client（DeepSeek） |
| **中间件** | `middleware/{request_id,device,strict_mode,maintenance,rate_limit}.rs` | — | 5 层，覆盖度详见 §2.4 |
| **Workers** | `workers/mod.rs` + 23 个 worker 文件 | — | `WorkerManager` 用 `tokio-cron-scheduler` 注册；`heartbeat_watchdog` 独立 tokio spawn，不归 Manager |
| **辅助** | `blocking.rs` | 110 | 全局 Semaphore（permits ≥ sqlite_pool_size）控并发 `spawn_blocking`，避免线程池耗尽 |
| **辅助** | `extractors.rs` / `response.rs` / `validation.rs` / `auth.rs` / `logging.rs` / `constants.rs` | — | 横切关注点 |
| **CLI bin** | `bin/migrate_sled_to_sqlite.rs` | 280+ | feature-gated（默认不编译），见 §3.3 |
| **CLI bin** | `bin/maimemo_mdm_adapter.rs` | — | MDM 离线评测 CLI |
| **独立 crate** | `crates/visual-fatigue-wasm/` | — | wasm-bindgen 视觉疲劳侦测，cdylib + rlib |

### 1.2 前端 Solid.js（`frontend/src/`）

| 组 | 路径 | 备注 |
|---|---|---|
| **框架** | `solid-js@1.9` + `@solidjs/router@0.15` | **不是 React**（主要状态原语：`createSignal` / `createResource` / `createMemo`） |
| **入口** | `main.tsx` | 挂 `QueryClientProvider`，但见 §2.3 |
| **路由** | `App.tsx` | 单 Router；用户路径 12 条全部落到 `LegacyUserFrontendPage`（"用户前端已迁移到独立仓库 wordforge-web"），admin 子路径 14 条 |
| **API client** | `api/client.ts` | 单一 fetch 封装：注入 `Authorization` / `X-Device-Id` / `X-Device-Platform`、401 自动 refresh、401 重试 1 次、SSE 通道独立维护 |
| **API 模块** | `api/{admin,amas,auth,client,content,health,learning,notifications,probe,records,studyConfig,userProfile,users,wordbookCenter,wordbooks,words,wordStates}.ts` | 17 个 |
| **页面** | `pages/admin/*.tsx` | 15 个 admin 顶层页面 + `pages/admin/amas/*.tsx` 9 个子面板（TierAPanel / VersionComparePanel / MetricsDashboard / AnomaliesPanel 等） |
| **页面** | `pages/{LegacyUserFrontendPage,MaintenancePage,NotFoundPage}.tsx` | 用户端只剩 1 个跳转占位（67 行） |
| **组件** | `components/{admin,auth,layout,probe,ui}` | UI 包 + AdminLayout + ProtectedRoute |
| **Stores** | `stores/{theme,ui}.ts` | **仅 2 个**：主题切换 + Toast 队列；admin 业务态全部走 `createResource` |
| **Lib** | `lib/{chartTheme,constants,device,fatigueWarningCooldown,motion,queryClient,storage,token,WordQueueManager}.ts` | 通用工具 |
| **Worker** | `workers/probe/api-bridge.ts` | 远程探针客户端桥（订阅 SSE → 沙箱执行 → 回传） |
| **测试** | `tests/*.test.*` | 116 个 vitest 单测，覆盖 admin 全部页面 + amas 子面板 + UI 组件 |
| **E2E** | `e2e/*.spec.ts` | 11 个 playwright 套件（admin / auth / records / wordbooks / learning-flow / notifications / profile / study-config / wordbook-center / home-navigation） |

---

## 2. 技术债 TOP 清单（按严重度排序）

### P0 · 必须在 v1 GA 前解决

#### P0-1 · `services/learning.rs` 是空壳，服务层抽象失败
`src/services/learning.rs:1-25`：`LearningService` 只持有 `store` + `amas` 两个 Arc，**没有任何业务方法**。学习相关的 1398 行业务逻辑全部留在 `src/routes/learning.rs`，路由层直接调 `state.amas()...` + `state.run_store_task(...)`，绕过本应隔离 IO 的 service 层。`AdminService` / `WordbookService` 同模式：仅 `admin/mod.rs` 调了 `state.admin_service().ban_user()` 等 3 个方法，`wordbook_center.rs:746` 调了 1 个 service 方法。**整个 services 层是死代码 + 命名误导**。

#### P0-2 · V1 路由静默退化为非自适应学习
`src/routes/v1.rs:1-11` 已有警告头注释（P3#6 加的），但仍存在以下行为：
- `POST /api/v1/records`（`src/routes/v1.rs:109-145`）只做 5 秒去重 + `store.create_record`，**不调 `state.amas().process_event`**，不更新 `user_state` / ELO / mastery / IGE / SWD / trust scores / monitoring。
- `POST /api/v1/learning/session`（`src/routes/v1.rs:184-208`）只 create-or-resume，**不计算 `cross_session_hint`**。
- 与 `src/routes/records.rs:290-311`（含完整 AMAS 调用） / `src/routes/learning.rs:265-377` 对比，功能差距巨大。
- middleware（`src/middleware/strict_mode.rs:42`）**整段豁免 `/v1/`**，UA/平台/最低版本一律不查，更易被误用。

iOS 客户端目前调 `/api/*`，无 v1 调用，但 v0.6 起已无 staged 客户端切换计划，应在 v1 GA 时**显式 410 Gone 或下架**，避免未来新客户端踩进静默退化坑。

#### P0-3 · `tokio-cron-scheduler` 调度器无死锁/卡死监测
`src/workers/mod.rs:266-289`：`WorkerManager::start` 调 `JobScheduler::new() → start() → shutdown_rx.recv() → drain → shutdown`。`add_job` 内置 overlap guard（CAS）+ `WORKER_TIMEOUT`（`src/workers/mod.rs:510-519`），但：
- **没有调度器健康度指标**：若 scheduler 内部 tick task panic 或被 tokio runtime drop，所有 worker 静默停摆，仅 `tracing::warn` 一行。
- `heartbeat_watchdog`（`src/workers/heartbeat_watchdog.rs:7-17`）**不归 `WorkerManager` 管**，由 `main.rs:127` 独立 `tokio::spawn` 出去；监控的是 SSE 设备掉线，与 cron 调度本身无关。
- 没有"上一次执行时间"上报到 admin 监控面板，admin 看不出哪个 worker 实际停了。

#### P0-4 · WIP / stub worker 永远 enabled=false，占着模块位
`src/workers/mod.rs:225-262`：
- `MonitoringAggregate`（行 222-227，cron `0 */15 * * * *`）`enabled: false` 注释 `WIP: 待监控聚合实现完成后启用`。
- `EtymologyGeneration`（`src/workers/etymology_generation.rs:1-48`）写假数据 `"Auto-generated etymology for '{}'"`，`enabled: false`。
- `EmbeddingGeneration`（`src/workers/embedding_generation.rs:1-29`）仅打 log 不写嵌入，文件首行 `TODO: 实现词向量嵌入生成 worker`。
- `WordClustering`（`src/workers/word_clustering.rs:1-70`）只统计难度分桶 + tag 计数，**没有聚类**，cron `0 0 4 * * SUN` 实际处于 disabled。

4 个永远启动不了的 worker 占着 schema 与配置，admin UI 也得反复解释"为什么这个 worker 不亮"。建议要么补齐要么删除（推荐删除）。

#### P0-5 · `frontend/src/lib/queryClient.ts` + `@tanstack/solid-query` 是 0 业务调用的死依赖
`frontend/src/main.tsx:3-18`：`QueryClientProvider` 已挂载，`lib/queryClient.ts:1-15` 配置了 staleTime 2 分钟 / gcTime 10 分钟 / retry 1。但全仓 `grep -rln "createQuery|createMutation" frontend/src/` **返回 0**：所有数据获取走 `createResource`（33 处）+ `createSignal` / 手写 polling（`App.tsx:66-78` 30 秒 status polling）。

QueryClient 闲置导致：
- 包体白白多 30~50 KB
- 新人会困惑"为什么 admin 页都不用 query"
- Toast 错误处理重复实现（每个页面自己接 ApiError）

要么删，要么把 admin 业务态迁过去（推荐迁，能消除手写 `loading + error + retry` 三联）。

#### P0-6 · `src/amas/engine.rs` 16 处 `RwLock::read/write.unwrap()` —— 锁中毒即整库不可用
`src/amas/engine.rs:104-535`：所有 `self.config.read().unwrap()` / `user_lock.lock().unwrap()` 在锁 poisoned 时 panic。其中：
- `process_event_blocking`（行 178）的 user_lock 持有期间若内部 panic（e.g. mastery 更新 panic），后续所有该用户事件全部死锁 panic。
- `config.write().unwrap()`（行 104）在 `reload_config` 路径，如果某个 reader panic 过一次，此后再也 reload 不了。

需要：把所有 `lock().unwrap()` 改成 `lock().unwrap_or_else(|e| e.into_inner())`（生产代码可接受的"中毒后继续"），或者改用 `parking_lot::RwLock`（无中毒概念）。同类问题在 `src/state.rs` 的 `update_cache: Arc<RwLock<...>>` 也存在。

#### P0-7 · `src/routes/records.rs` AMAS 事务非原子，回滚是手抖的
`src/routes/records.rs:660-687`：批量写入失败时手动 rollback 3 个状态（`engine_algo_state` / `word_elo` / `user_elo`），rollback 失败仅打 warn 不报错。**没有跨 store 操作的事务包裹**，且 `state.amas().process_event` 走的是 `AMASEngine` 内部锁，与 store 事务完全独立 —— 如果 amas 写完 ELO/mastery 后 records.create_record 失败，新状态留下了脏数据。

需要：要么把 records 写入 + ELO/mastery 更新合并到同一 `store.write_in_tx` 闭包，要么明确"AMAS 状态最终一致 + 业务事件可回放"的契约。

### P1 · v1 不解决就有运维负担

#### P1-1 · `src/routes/learning.rs` 1398 行 + `src/routes/records.rs` 849 行，单文件膨胀
`learning.rs` 含 10 个 handler + `process_single_record` / `process_batch_record` 内部辅助；`records.rs` 含批量场景。建议按 session / next-words / sync / complete / pick / generate-options 拆 4-5 个子文件。同病：`routes/admin/amas.rs` 988 行 / `analytics.rs` 724/675 行 / `wordbook_center.rs` 1083 行。

#### P1-2 · 14 条 migration 跳号且无回滚
`src/store/migrate.rs:1-25`：`m013_learning_record_self_rating` 在 `m012` 之前出现在数组（但写入 schema_version 时按 index+1，所以 m013 实际是版本 13），加上 `m010` 写在 `m011` 之后（行 304/324），人工排序极容易引入"漏跑"。`set_version` 只拦下降级，没有 down migration，回滚靠 backup。

#### P1-3 · `store/operations/extras.rs` 1134 行 catch-all
`store/operations/extras.rs` 是其他模块没收下的零散查询合集，命名提供 0 信息。新增表往往直接塞这里 → 滚雪球。

#### P1-4 · `store/keys.rs` 是 sled 时代遗物
`src/store/keys.rs` 仅服务于 `bin/migrate_sled_to_sqlite.rs`（feature `sled-migration`）。生产代码引用了里面的 `learning_session_key` 等几个常量（`src/store/operations/learning_sessions.rs:8`），删除前需要先把这些常量迁出来。**sled-migration feature 默认不构建，CI 不跑**，意味着该 binary 长期不被验证。建议 v1 删 binary + feature + crate `sled` 依赖。

#### P1-5 · strict-mode 中间件豁免列表脆弱
`src/middleware/strict_mode.rs:40-47`：硬编码 4 条豁免（`/admin/`、`/v1/`、`/status`、`/realtime/events`）。新增 admin SSE 端点（如 `/admin/updates/events`）走 admin 子树自动豁免，但**非 admin SSE 端点如新增**就会被误拦。建议改为路由元数据或 router builder 注入豁免标记。

`src/middleware/maintenance.rs:14-18` 同样硬编码豁免 4 条（`/admin/`、`/status`、`/realtime/`、`/telemetry`），与 strict-mode 列表**不一致**（strict-mode 漏了 `/telemetry`；maintenance 漏了 `/v1/`），潜在前端"维护中 telemetry 仍能传 / v1 在维护时也接 record"的语义错位。

#### P1-6 · `frontend/src/components/ErrorBoundary.tsx:14` 未接 Sentry
注释 `// TODO: 集成 Sentry / 监控时在此 hook`。线上崩溃靠用户截图 + admin/feedback 入口，没有任何 client-side 错误回流。

#### P1-7 · 测试豁免与 ignore
- Rust：`tests/perf_workers_compare.rs:260` `#[ignore]` 性能基准；`src/amas/memory/ssp.rs:708/789/825` 3 个长跑 SSP 验证（合理，需 `--release`）；`src/amas/config/tests.rs:452/527` 2 个 TOML 全量校验（已被其他测试覆盖）。**没有伪装的 broken test**。
- Frontend：`frontend/tests/pages/admin/UpdatesPage.features.test.tsx:250/275` 两条 polling 测试 `it.skip`（写注释看不出原因）；`AdminLoginPage.features.test.tsx:92` `it.skip` 三次失败后计数文案 —— **3 条 skip 都没 issue link**，应在 v1 收尾时复活或删除。

### P2 · 体感差但不影响 GA

#### P2-1 · `src/routes/health.rs:140` `// TODO: real error tracking not yet implemented`
health 端点的 error_rate 字段当前返回 0（占位），admin 监控曲线上一直是平的。

#### P2-2 · `frontend/src/pages/admin/amas/AnomaliesPanel.tsx:112` ECharts resize TODO
`{/* TODO(Group F): EChart need resize on height prop change */}` —— 在 topViolationFields 数量变化时图表高度不会重算。

#### P2-3 · `src/routes/v1.rs` 仅 4 个端点是真兼容层，其余靠"GET /api/v1/* 与 /api/* 等价"承诺
注释里这么写（行 7），但**实际没有任何代理层**实现这个等价 —— `routes/mod.rs:90` 就 `nest("/v1", v1::router())` 一句。如果未来移动端真发 `GET /api/v1/wordbooks`，会 404 而不是 fallthrough。

#### P2-4 · `src/amas/word_selector.rs` 711 行存在但未被 routes 调用
`grep -rn "select_words\|WordSelector" src/routes/` 0 命中。`word_selector::select_words` 只在 `amas/engine.rs` 内被 `pick_next_word` 调到（per AMAS 模块自洽）。`src/routes/learning.rs:42` 的 `/pick-next-word` 路由是另一套实现 —— 路由层 word selector 与 AMAS engine 内部 word selector **是两条路**。

---

## 3. 半成品 / 临时绕过 / 已知缺口

### 3.1 V1 路由刻意绕过 AMAS（原因 + 补齐方案）

**原因**（按代码注释 + git log d0325f8 推断）：
- v0.5.x 时 iOS 客户端是少量灰度新版 + 大量旧版混跑，旧版用 `/api/v1/*` 形态发请求；当时 AMAS 引擎还没冻结接口，怕旧客户端的"非标准 payload"（无 session_id / 无 self_rating / 无干预指标）触发引擎异常或污染指标，所以 v1 做成"只落日志不更新模型"。
- 现状（v0.6.0-beta.3）：iOS 已 100% 切到 `/api/*`，v1 路径**线上零调用**（参见 [[wordforge_client_backend_alignment_2026_05_19_v3]] 提到的契约 100% 对齐）。

**v1 GA 的三个补齐选项**：
1. **删除**（推荐）。v1.0 release notes 标注 deprecated → v1.1 删除 router 挂载 → 保留 410 Gone 占位 endpoint 半年。
2. **补齐**到与 `/api/*` 同语义。成本：v1 路由要走完 `process_event` + ELO + mastery + `assemble_word_state_update`，等于把 v1 重做成主端点副本，违反 P3#6 警告的初衷。
3. **保留兼容但增告警**。每次 v1 调用上报 telemetry，记录调用方 UA / device_id，30 天没新调用方则进入 deprecated 流程。

### 3.2 sled → sqlite 迁移残留

- `Cargo.toml` 仍有 `sled = { version = "=0.34.7", optional = true }` + feature `sled-migration`。
- `src/bin/migrate_sled_to_sqlite.rs:11-280` 完整保留，但 CI 不构建（feature gated）。
- `src/store/keys.rs` 中的 key 拼装常量仍被生产代码引用（`learning_sessions.rs:8`、其他 ops 文件），是真正阻碍清理的死结。
- `data/learning.db` + `learning.db-shm` + `learning.db-wal` 是当前活跃 SQLite，没有 sled 残留文件。

**结论**：迁移本身已完成，但代码层"半净"——binary 与 feature 还在，删除收益是少量 build matrix + 少量行数。建议 v1 之后一次性清。

### 3.3 LegacyUserFrontendPage 的实际身份

`frontend/src/pages/LegacyUserFrontendPage.tsx:1-67`：当 `VITE_USER_APP_URL` 未配置时，显示"用户前端尚未上线，请联系管理员"+"打开管理后台"按钮。意味着 v0.6 起，**项目仓库不再托管用户端 UI**，仅做 API + admin。`App.tsx:95-105` 12 条用户路径全部挂到这个占位页。

`docs/v1-research/v1` 立项时需明确：用户端 webview 是否归 wordforge-web 独立仓库，本仓库只承诺 API + admin 后台 GA。

### 3.4 4 个 stub worker（详见 P0-4）

`MonitoringAggregate` / `EtymologyGeneration` / `EmbeddingGeneration` / `WordClustering`。

### 3.5 `services/` 层 3 个空壳

- `LearningService` 25 行，0 业务方法。
- `WordbookService` 79 行，2 个方法。
- `AdminService` 225 行，4 个方法被 admin/mod.rs 调用，admin 子路由（analytics / broadcast / clients / monitoring / probe / settings / updates）**全部直接 `state.run_store_task(...)`**，没经过 service 层。

### 3.6 TODO/FIXME 完整清单（5 处生产代码）

| 文件:行 | 内容 | 严重度 |
|---|---|---|
| `src/workers/embedding_generation.rs:1` | TODO: 实现词向量嵌入生成 worker | P0-4 已覆盖 |
| `src/routes/health.rs:140` | TODO: real error tracking not yet implemented | P2-1 |
| `src/workers/mod.rs:225` | WIP: 待监控聚合实现完成后启用 | P0-4 |
| `src/workers/mod.rs:246/252/260` | WIP: 待 LLM provider 就绪后启用 ×3 | P0-4 |
| `frontend/src/components/ErrorBoundary.tsx:14` | TODO: 集成 Sentry / 监控 | P1-6 |
| `frontend/src/pages/admin/amas/AnomaliesPanel.tsx:112` | TODO(Group F): EChart need resize | P2-2 |

### 3.7 测试 skip / ignore 全清单

| 文件:行 | 类型 | 原因 |
|---|---|---|
| `tests/perf_workers_compare.rs:260` | `#[ignore]` | 性能基准，手动跑 |
| `src/amas/memory/ssp.rs:708/789/825` | `#[ignore]` | SSP 长跑参数扫描 |
| `src/amas/config/tests.rs:452/527` | `#[ignore = ...]` | 已被其他测试覆盖 |
| `frontend/tests/pages/admin/UpdatesPage.features.test.tsx:250/275` | `it.skip` | **无原因注释**（疑似 polling test 不稳定） |
| `frontend/tests/pages/admin/AdminLoginPage.features.test.tsx:92` | `it.skip` | **无原因注释**（count-aware 文案） |

3 条前端 skip 是技术债。

### 3.8 生产代码 `unwrap()` / `panic!` 统计

- `src/amas/engine.rs`：16 处（全部为 `Arc<Mutex/RwLock>::lock/read/write.unwrap()`，锁中毒即 panic，见 P0-6）
- `src/routes/auth.rs:300`：1 处 `user.unwrap()`（前面已 `if user.is_none() return Err`，逻辑安全但脆）
- `src/routes/admin/auth.rs:211`：同上模式
- `src/routes/learning.rs:1373`：`fallback_distractors.pop().unwrap()`（在 `!fallback_distractors.is_empty()` 之后，安全）
- `src/workers/daily_aggregation.rs:12`：`and_hms_opt(0, 0, 0).unwrap()`（固定常量，永远 Some）
- `src/config.rs`：12 处 `panic!`，全部为启动时配置校验（JWT_SECRET 长度 / CORS 解析 / 弱默认值），fail-fast 合理。
- `src/main.rs`：3 处 `panic!`，启动期 fail-fast。
- `src/logging.rs`：2 处 `panic!`，tracing 初始化失败。

**结论**：除 P0-6 锁中毒外，其他 `unwrap` / `panic!` 都是"启动期 fail-fast" 或"已显式校验后的兜底"，可接受。

---

## 4. v1 三大目标 × 模块 gap matrix

| 模块 | 稳定性 GA | 半成品补齐 | 架构升级 |
|---|---|---|---|
| **`store/`** | rusqlite 单库 OK 至 1k DAU；缺锁中毒防护 | sled 残留清理（P1-4） | 读副本 / 分表（如 `learning_records` → 按月分区） |
| **`store/operations/extras.rs`** | — | 切分该 1134 行 catch-all（P1-3） | — |
| **`amas/engine.rs`** | RwLock 中毒 panic 风险（P0-6） | — | engine 拆 read-path / write-path 两个 actor，避免 user_lock 内做 IO |
| **`routes/v1.rs`** | 静默退化（P0-2） | 删除或对齐（§3.1） | — |
| **`routes/learning.rs`** | 1398 行单文件（P1-1） | — | 按 lifecycle 拆 4-5 子文件 |
| **`routes/records.rs`** | 跨 amas/store 非原子（P0-7） | — | 引入领域事件总线（write → emit ⇒ amas/elo/word_state 异步消费） |
| **`services/*.rs`** | 命名误导（P0-1） | 要么删 LearningService，要么把 routes/learning.rs 业务下沉 | — |
| **`middleware/strict_mode.rs`** | 豁免列表硬编码（P1-5） | — | 改路由元数据驱动 |
| **`middleware/maintenance.rs`** | 豁免列表与 strict-mode 不一致（P1-5） | — | 同上 |
| **`workers/mod.rs`** | 调度器卡死无监测（P0-3） | 删 4 个 stub worker（P0-4） | worker 状态 + last_run_at 入 admin 监控 |
| **`workers/heartbeat_watchdog`** | 在 `main.rs` 独立 spawn，不归 Manager | — | 归并 WorkerManager 统一生命周期 |
| **`bin/migrate_sled_to_sqlite.rs`** | — | 删（§3.2） | — |
| **`crates/visual-fatigue-wasm`** | wasm 产物未在 frontend build 流水线说明 | — | 仅 1 个 crate，cdylib + rlib，文档缺 |
| **`frontend/main.tsx` + `lib/queryClient.ts`** | 死依赖（P0-5） | 删除 or 迁移业务态 | 迁移到 @tanstack/solid-query → 统一 loading/error |
| **`frontend/stores/`** | 仅 2 个 store，业务态全靠 createResource，OK | — | 不变；若引 query 则 store 进一步退化 |
| **`frontend/src/components/ErrorBoundary.tsx`** | 缺 Sentry hook（P1-6） | 接 Sentry / openobserve | — |
| **`frontend/pages/admin/amas/`** | 9 个子面板 + 1 个 EChart resize TODO | — | — |
| **`frontend/pages/LegacyUserFrontendPage.tsx`** | 占位页，确认本仓库不托管用户端 | 文档化"v1 = API + admin"边界（§3.3） | — |
| **`frontend/e2e/`** | 11 个 spec 覆盖 admin + auth + records + wordbooks + learning-flow + notifications + profile + study-config + wordbook-center + home-navigation，**用户端流程 e2e 仍在本仓** | 与 wordforge-web 仓库的 e2e 边界要划清 | — |
| **测试** | 3 条前端 it.skip 未跟进（P1-7） | 复活或删除 | — |

---

## 5. 架构升级候选列表

按"v1 是否必须 / 投入 / 收益"三轴排序：

### 5.1 v1 GA 内建议落地（高 ROI · 低风险）

1. **`services/` 层做减法**：删 `LearningService` / `WordbookService`，`AdminService` 也整合进 `state.rs` 的 store helpers。命名上承认"本仓库没有 service 层，只有 route handler + store ops"，避免新人误以为有 DDD 分层。
2. **`v1.rs` 路由 410 Gone**：v1 GA 同步弃用，6 个月窗口后删除。
3. **`workers/mod.rs` 删 4 个 stub**：`MonitoringAggregate` / `EtymologyGeneration` / `EmbeddingGeneration` / `WordClustering`。如果未来需要语义聚类，应明确归到 LLM provider 接入后的 v1.x 路线，而不是占着代码位。
4. **`store/keys.rs` + `bin/migrate_sled_to_sqlite.rs` + sled feature 删除**：把 `learning_sessions.rs:8` 等几个 key 常量迁到 ops 内联，然后一次性砍 sled 整条链。
5. **`AMASEngine` 锁中毒防护**：把 `*.lock().unwrap()` 换 `lock().unwrap_or_else(PoisonError::into_inner)` 或 `parking_lot` 锁。最小补丁 < 50 行。
6. **`frontend/main.tsx` queryClient**：要么删 dependency（30~50KB），要么开始迁第 1 个 admin 页面（推荐先迁 ClientsPage / AnalyticsPage 这种纯 list）。

### 5.2 v1.x（GA 后 3 个月内）建议

7. **`workers` 调度可观测**：每个 worker 上报 `last_run_at` / `last_duration_ms` / `last_error` 到 `system_settings` 同等的 singleton 表，admin/monitoring 面板加一列；scheduler 自身加 health gauge。
8. **`routes/learning.rs` 拆分**：1398 行按 session lifecycle 切 4 个文件（`session.rs` / `study.rs` / `progress.rs` / `pick.rs`），同步消除单文件 `mod tests` 1000 行。
9. **`routes/records.rs` 引入事件总线**：把 `process_event` 改成"先 commit 记录、再发事件、AMAS 异步消费"，rollback 困境从此消失。代价是引入 in-process channel 或 outbox 表。
10. **strict-mode / maintenance 豁免改路由元数据**：用 `Router::route_layer` 或 axum 0.7 的 `route_with_metadata` 模式，避免硬编码 path 前缀。

### 5.3 v2 / 长期候选（v1 不动）

11. **读副本 / 分库**：`learning_records` 已经成为最大表，按月分区或独立 store handle。
12. **AMAS engine actor 化**：当前 user_lock 内做 IO 可能阻塞其他事件；可以拆 read-only path（strategy 计算）和 write path（process_event）到独立 task pool。
13. **前端 React 化 / 单仓 monorepo**：当前 Solid + 本仓 admin + wordforge-web 独立用户端，长期维护成本高于收益的话再考虑统一。
14. **多实例 worker 协调**：当前靠 `WORKER_LEADER=true` env 选主，是单点；如果未来要 HA 部署，需要 lease（Redis / 数据库行锁）。

---

## 6. 一句话总结

WordForge 当前是 "**单实例 axum + sqlite + AMAS 引擎 + admin Solid SPA**" 的紧凑栈，模块边界总体清晰；最大的真实债是 **`services/` 层空壳**（误导）、**`/api/v1/*` 静默退化**（潜在事故源）、**4 个 stub worker**（占编 4 个文件 + 4 个 schema 字段没填）、**前端死依赖 QueryClient**（30~50KB 浪费 + 0 业务收益）、以及 **AMASEngine 16 处 lock-unwrap**（锁中毒 panic 风险）。这五项在 v1 GA 内全部可解，单项成本 < 1 天，合计 ≤ 1 周可清空。
