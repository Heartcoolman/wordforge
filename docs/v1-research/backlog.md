# WordForge v1.0 Backlog

> 起草日期：2026-05-21
> 配套 RFC：`docs/v1-research/RFC.md`
> 颗粒度：每条 ≤ 2 人日（超出必拆）；可直接拆为 GitHub issue。
> 字段：**维度** = arch/signal/perf/release（来源 agent）；**优先级** = P0 (MUST) / P1 (MUST 细分) / P2 (SHOULD)；**估时** 单人日，含测试与文档；**源** = 调研报告章节锚。

## 总览

| 里程碑 | 任务数 | 总估时 | 触发 |
|---|---|---|---|
| M0（→ rc.1，基础修复 + 安全网）| 23 | ≈ 11.4 人日 | RFC §6.1 |
| M1（→ rc.2，代码债 + 合规）| 10 | ≈ 9.7 人日 | RFC §6.2 |
| M2（→ rc.3，质量门验证）| 4 | ≈ 5 人日 | RFC §6.3 |
| S（SHOULD，可并行不阻塞 GA）| 9 | ≈ 11 人日 | RFC §4.2 |
| **GA 必经合计（M0+M1+M2）**| **37** | **≈ 26 人日** | RFC §6 |

---

## M0 · 基础修复 + 安全网（→ v1.0-rc.1）

### M0-C · 契约 / 序列化

#### M0-C1 · 客户端×后端第四轮 cross-validator 审计
- **维度**：release / signal
- **估时**：1.0 人日
- **依赖**：无
- **描述**：把 v0.6.0-beta.1–beta.4 期间的变更（双通道、ErrorBoundary 修、release notes md 渲染、WordState wire lowercase、favorites paginated）合入 `docs/alignment.md`；跑 cross-validator 输出 P0/P1 数；如发现 P0 ≥ 1 项必须修复后再发 rc.1。
- **验收**：`docs/alignment.md` 更新至 v0.6.0-beta.4 现状；P0 = 0 / P1 = 0；CI `cargo test` 全绿。
- **源**：release-keeper §3.1；signal-miner §1 项 10

#### M0-C2 · 引入 utoipa 注解链 + OpenAPI 自动导出 + CI 防漂移
- **维度**：release
- **估时**：2.0 人日
- **依赖**：无
- **描述**：先覆盖 v1 stable 档约 25 个端点（详见 release-keeper §3.2.1）。集成 `utoipa` + `utoipa-axum`，build.rs 或 `cargo test --test openapi_export` 把 `docs/openapi.yaml` 写盘；CI 加 `git diff --exit-code docs/openapi.yaml` 防漂移；保留 schemars JSON Schema 给 AMAS config。**决策点 O2 选 (b)** → 先 stable 档。
- **验收**：`docs/openapi.yaml` version 与 `Cargo.toml.version` 一致；`cargo test --test openapi_export` 通过；`docs/api-endpoints.md` 章节顺序与 openapi.yaml `paths` 顺序一致；CI 改 openapi 不同步会失败。
- **源**：release-keeper §1.5 / §4.4 / §6.1 C2；用户拍板 D4

#### M0-C3 · SSE 事件表补齐 4 个缺失事件
- **维度**：release
- **估时**：0.5 人日
- **依赖**：无
- **描述**：`docs/api-endpoints.md:2353-2371` SSE 表补 `new_llm_suggestion / release_available / update_progress / probe_request / probe_confirm`；同步修 `update_available` 旧 payload 示例；在 utoipa schema 中也表达（AsyncAPI 风格 description 或单独 `events.md`）。
- **验收**：9 个 SSE 事件全表；后端 `src/state.rs:25-84` 与文档名称一一对应；`tests/sse_event_table.rs` 集成测试通过（断言 SseEvent 变体数 = 文档项数）。
- **源**：release-keeper §1.5 / §6.1 C3

#### M0-C4 · `Deprecation` / `Sunset` header 中间件
- **维度**：release
- **估时**：0.5 人日
- **依赖**：无
- **描述**：实现中间件，从路由元数据读取 `deprecated_since` / `sunset_date` 并写入响应头（RFC 8594）；handler 加 `#[deprecated]` 标记是声明源；为 `/api/admin/monitoring/check-update` 与 `/api/v1/*`（在 M0-C5 后）打头阵。
- **验收**：`tests/deprecation_header.rs` 集成测试通过；admin 控制台访问 `/admin/updates` 网络面板能看到 `Deprecation:` / `Sunset:` header 出现在 deprecated 端点上。
- **源**：release-keeper §6.1 C4 / §3.3

#### M0-C5 · `/api/v1/*` 立即 410 Gone（已拍板 D2）
- **维度**：arch / release
- **估时**：0.5 人日
- **依赖**：M0-C4（用 Deprecation header 标记）
- **描述**：`src/routes/v1.rs` 4 个端点全部返回 410 Gone + JSON `{ "error": "GONE", "message": "...", "sunset": "<v1.0 发布日 + 12 个月>" }`；`routes/mod.rs:90` 保留 `nest("/v1", v1::router())` 让 404 不致；release notes 公告 deprecated；`api-endpoints.md` 第 18 节加红色 banner。
- **验收**：`curl /api/v1/records` 返回 410；`docs/api-endpoints.md` 与 release notes 同步公告。
- **源**：arch-scout P0-2 / §3.1；signal-miner §5.4；用户拍板 D2

#### M0-C6 · 修 `verify-*-auto-update*.sh` 双通道契约
- **维度**：release
- **估时**：0.5 人日
- **依赖**：无
- **描述**：`scripts/verify-release-auto-update.sh:118-127` 改用 `{stable, beta}` 嵌套契约 + apply 必带 `channel`；同时删除 `cargo build --release` 现编步骤（违反"禁服务器编译"规则），改为下载 release artifact。（`scripts/verify-auto-update-v043.sh` 及其 `verify-auto-update-v043.yml` 已于 commit `a022e6b` 删除，无需再改。）
- **验收**：`verify-auto-update-v044` 分支 CI 跑通 v0.6.0-beta.X → v0.6.0-beta.Y 升级冒烟。（旧 `verify-auto-update-v043.yml` 已删除 = 显式 deprecate，此条已落地。）
- **源**：release-keeper §1.5 / §6.1 C6

### M0-R · 发布流

#### M0-R1 · `release.yml` 加 `pre-release-tag-lint`
- **维度**：release
- **估时**：0.2 人日
- **依赖**：无
- **描述**：在 release.yml 第一个 job 前加 lint step：tag 含 `-` 必须匹配 `^v\d+\.\d+\.\d+-(alpha|beta|rc)\.\d+$`，否则 fail；不含 `-` 必须匹配 `^v\d+\.\d+\.\d+$`。
- **验收**：故意打错 tag（如 `v1.0.0-foo` 或 `v1.0`）触发 lint failure；现有 tag 列表全通过。
- **源**：release-keeper §1.3 / §6.2 R1；signal-miner §5.3

#### M0-R2 · minisign 签名 + updater 验签（已拍板 D6）
- **维度**：release / perf
- **估时**：1.0 人日
- **依赖**：无
- **描述**：release.yml 加 minisign 签名 step（私钥从 GH secret）；签名文件随 tarball / sha256 一并发布。updater 端 `src/services/updater.rs` 在 `fetch_sha256` 后追加 `fetch_signature` + `verify_minisign`；公钥嵌入二进制（**决策 O5 选 (a)** 编译期常量，build.rs 注入）。
- **验收**：rc.X 发布手动测试：把 tarball 篡改后 updater apply 拒绝；`docs/auto-update.md` 加签名校验流程图；私钥泄露应急流程入 `docs/runbook/key-rotation.md`（M0-D7 副产）。
- **源**：release-keeper §2.2 P2 / §6.2 R2；用户拍板 D6

#### M0-R3 · 自更新失败自动回滚 + systemd unit 入仓
- **维度**：release / perf / signal
- **估时**：1.5 人日
- **依赖**：无
- **描述**：① fork-exec 后 60 秒内子进程必须 `GET /health` 返回 200，否则父进程 swap 回 `wordforge.{old_tag}` 并打 admin SSE 告警；② `deploy/wordforge.service.tmpl` 入仓（systemd unit 模板），`install.sh` 引用模板而非内嵌；③ install.sh 默认 `Restart=always`（修第 4 坑点），新部署不会因 fork-exec 父退 0 卡死。
- **验收**：集成测试故意启动 panic 的子进程，观察 swap 回 + admin SSE 收到告警事件；新部署机器 systemctl restart 表现稳定。
- **源**：feedback_admin_self_update_pitfalls 第 4 坑；release-keeper §2.2 / §6.2 R3；signal-miner §5.6

#### M0-R4 · apply swapping 自动开 maintenance 模式
- **维度**：release
- **估时**：0.5 人日
- **依赖**：M0-R3
- **描述**：`src/services/updater.rs` apply 状态机进 `swapping` 时通过 `state.set_maintenance(true)`；进 `completed` 或 `failed` 时关闭。**注意 fork-exec 后父进程没机会 reset**，由新进程启动时检查 `maintenance.flag` 文件清理。
- **验收**：集成测试：apply 期间外部 `GET /api/health` 返回 503 + body `{ "maintenance": true }`；apply 完成后 200。
- **源**：release-keeper §2.2 P1 / §6.2 R4

### M0-D · 文档

#### M0-D1 · 项目根 `README.md`
- **维度**：release
- **估时**：0.5 人日
- **依赖**：无
- **描述**：项目简介 + 截图 / 架构图 + 一键安装命令 + 文档站链接 + 许可证 + 贡献提示。中文为主，英文段落简介在顶部（便于 GitHub 推荐）。
- **验收**：GitHub 仓库首页非空；包含至少 1 张架构图（可复用 `docs/guide/architecture.md` 的图）；安装命令可复制粘贴跑通。
- **源**：release-keeper §4.3 / §6.3 D1

#### M0-D2 · `CHANGELOG.md`（v0.1.2 → v0.6.0-beta.4 全量）
- **维度**：release
- **估时**：1.0 人日
- **依赖**：无
- **描述**：写一次性脚本 `scripts/build-changelog.sh`，解析 GitHub release notes API → 归档到 `CHANGELOG.md`；遵循 Keep a Changelog 格式；后续每发版手动 append。
- **验收**：`CHANGELOG.md` 包含所有历史 tag；`docs/.vitepress/config.mts` 加入侧栏。
- **源**：release-keeper §6.3 D2

#### M0-D3 · `SECURITY.md`
- **维度**：release
- **估时**：0.2 人日
- **依赖**：无
- **描述**：安全披露通道（邮箱 / GH security advisory）+ 漏洞响应窗口（48h 确认 / 30 天修复目标）+ 公开致谢机制 + 默认不接受高危漏洞披露后立即公开。
- **验收**：仓库顶级文件存在；GitHub Security tab 自动识别。
- **源**：release-keeper §6.3 D3

#### M0-D4 · `CONTRIBUTING.md`
- **维度**：release
- **估时**：0.5 人日
- **依赖**：无
- **描述**：Conventional Commits 约束 + PR 模板（`.github/PULL_REQUEST_TEMPLATE.md`）+ 本地三件套跑法（`cargo test` + `npm test` + `npm run test:e2e`）+ 代码风格 + 提交前 checklist（lockfile / a11y / 二次确认 e2e / 表单 disabled 表达式，对应 [[feedback_release_pre_flight_checks]]）。
- **验收**：顶级文件存在；PR 模板生效。
- **源**：release-keeper §6.3 D4；feedback_release_pre_flight_checks

#### M0-D5 · 更新 `docs/auto-update.md`（双通道完整版）
- **维度**：release
- **估时**：0.5 人日
- **依赖**：M0-R2（minisign 流程要写进去）/ M0-R3 / M0-R4
- **描述**：补 `channel` 参数、`{stable, beta}` 嵌套响应、异步 apply + `applyTask` 轮询模型、minisign 验签步骤、自动回滚机制、maintenance 模式自动切换、phase 超时（M0-P5）。修正 systemd unit 描述与 install.sh 实际行为对齐。
- **验收**：文档与 `src/services/updater.rs` + `src/routes/admin/updates.rs` 字段名 / 字段顺序一一对齐。
- **源**：release-keeper §4.2 / §6.3 D5

#### M0-D6 · VitePress sidebar 收录运维 / 开发者参考
- **维度**：release
- **估时**：0.2 人日
- **依赖**：M0-D5, M0-D7
- **描述**：`docs/.vitepress/config.mts` 加 "运维 Runbook" 与 "开发者参考" 两类侧栏，收录：`auto-update.md` / `alignment.md` / `amas-schema-codegen.md` / `openapi.yaml` / `ui-audit.md` / `amas-admin-console.md` / `runbook/*` / `user/*`。
- **验收**：本地 `npm run docs:dev` 检查侧栏完整；GitHub Pages 部署后顶部 nav 三大类（用户 / 运维 / 开发者）齐全。
- **源**：release-keeper §4.6 / §6.3 D6

#### M0-D7 · `docs/runbook/` 5 篇运维 SOP（决策 O1 选 a 一次性）
- **维度**：release / perf
- **估时**：3.0 人日（5 × 0.6 平均）
- **依赖**：M0-R3, M0-R4, M0-P1
- **描述**：5 篇 runbook：
  - `backup-restore.md`（DB 备份 + 灾恢，含 VACUUM INTO + 异步外推可选）
  - `incident-response.md`（5xx 上涨 / SSE 打满 / GitHub rate-limited / 磁盘满 / WAL 不收 的诊断步骤）
  - `key-rotation.md`（JWT / Admin / Refresh / minisign 私钥轮转 SOP）
  - `scaling.md`（容量上限信号 + 何时考虑切 Postgres 的预警阈值）
  - `monitoring-setup.md`（对接外部 Prometheus / Alertmanager 的配置示例）
- **验收**：5 篇全成；每篇含"症状 → 诊断 → 处置"三段式；侧栏收录。
- **源**：release-keeper §5 / §6.3 D7

#### M0-D8 · `docs/user/` 4 篇最终用户文档
- **维度**：signal / release
- **估时**：2.0 人日
- **依赖**：无
- **描述**：4 篇用户文档（即使学习端在 wordforge-web 仓，本仓也是 API 接入参考站）：
  - `installation-ios.md`（TestFlight 安装 / 自有客户端编译）
  - `installation-web.md`（指向 wordforge-web 仓 + 自托管参数）
  - `faq.md`（常见同步失败 / 注册问题）
  - `privacy.md`（GDPR 合规说明 + 数据导出/删除入口指引，配合 M1-G1）
- **验收**：4 篇全成；侧栏收录；`docs/.vitepress/nav` 顶部"用户"分类点开可见。
- **源**：release-keeper §4.1 / §6.3 D8；signal-miner §3.4

### M0-P · 性能 / 监控

#### M0-P1 · `/metrics` 端点（Prometheus / OpenMetrics）
- **维度**：perf / release
- **估时**：2.0 人日
- **依赖**：无
- **描述**：集成 `axum-prometheus`（或自实现 tower-http MakeHistogram）；导出 `http_request_duration_seconds{route, method, status}` + `sse_active_connections` + `db_size_bytes` + `worker_last_run_seconds{name}` + `amas_process_event_duration_seconds`；端点 `/metrics` 用 admin 鉴权（同 `/api/admin/*` 鉴权链）。
- **验收**：`curl -H "Authorization: Bearer <admin>" /metrics` 返回 Prometheus 文本格式；含 ≥ 10 个有意义的 metric；`tests/metrics_endpoint.rs` 通过；`docs/runbook/monitoring-setup.md`（M0-D7）引用本端点。
- **源**：perf-warden §6.1 P0#1 / §1.3；release-keeper §4.2 O1

#### M0-P2 · `.env.example` 与代码默认对齐 + 发版前自检
- **维度**：perf
- **估时**：0.5 人日
- **依赖**：无
- **描述**：① 同步 `.env.example` 所有项与 `src/config.rs:280-470` 默认值一致（重点 `SQLITE_POOL_SIZE=16`）；② 加 `scripts/verify-env-example.sh` 在 release.yml pre-job 跑；③ [[feedback_release_pre_flight_checks]] 内存追加第 4 条"env.example sync"。
- **验收**：`scripts/verify-env-example.sh` 退出 0；故意改 `.env.example` 出现一处不一致退出 1；release.yml 集成该 check。
- **源**：perf-warden §5.2 P1 / §6.1 P0#2；signal-miner §5.7

#### M0-P3 · `monitoring_aggregate` worker 实装或 retention
- **维度**：perf / arch
- **估时**：1.5 人日
- **依赖**：无
- **描述**：选择路径 (a) 实装聚合（按 user × hour 聚合 engine_monitoring_events → engine_monitoring_aggregate 表，删源记录 > 30 天）或 (b) 仅加 retention worker 删 > 30 天 raw 记录。**推荐 (b) + 加 cron 月度 VACUUM**，因 (a) 设计未冻结。
- **验收**：`engine_monitoring_events` 表大小可控；`docs/runbook/scaling.md`（M0-D7）记录 retention 策略。
- **源**：arch-scout P0-4；perf-warden §5.2 P10 / §6.1 P0#3

#### M0-P4 · 5xx 错误率告警接 admin SSE
- **维度**：perf / release
- **估时**：1.0 人日
- **依赖**：M0-P1
- **描述**：新 worker `error_rate_watchdog` cron 1 分钟，读取 M0-P1 暴露的 `http_request_duration_seconds_count{status=~"5.."}` / total，5xx 滚动 5min > 1% → admin SSE 推 `incident` 事件；admin UI `/admin` Dashboard 加 incident badge。
- **验收**：构造 5xx 注入测试触发告警；admin 控制台收到 SSE event；同一告警 5 分钟内不重复推。
- **源**：perf-warden §6.1 P0#4

#### M0-P5 · 自更新 phase 超时
- **维度**：release / perf
- **估时**：0.5 人日
- **依赖**：M0-R3
- **描述**：apply 状态机每个 phase（downloading / verifying / extracting）独立 watchdog：> 5 min 不进入下一 phase 主动 abort + rollback；abort 触发 M0-R3 的回滚链路。
- **验收**：集成测试模拟 GitHub 慢链路 > 5min 触发 abort；用户在 admin UpdatesPage 看到明确错误 + 自动回滚成功。
- **源**：perf-warden §5.2 P7 / §6.1 P0#5

---

## M1 · 代码债 + 合规（→ v1.0-rc.2）

### M1-A · 架构 / 代码债

#### M1-A1 · 删除 `LearningService` + `WordbookService`；`AdminService` 整合
- **维度**：arch
- **估时**：1.5 人日
- **依赖**：M0 全部完成（避免在 rc.1 修复中插入大重构）
- **描述**：删 `src/services/learning.rs` + `src/services/wordbook.rs`；`src/services/admin.rs` 4 个方法（list_users / ban_user / stats / set_user_password）迁到 `src/state.rs` helpers 或直接落 `src/routes/admin/*.rs`；`AppState` 移除 `learning_service` / `wordbook_service` / `admin_service` 字段；命名上承认本仓没有 service 层，避免新人误判 DDD 分层。
- **验收**：`cargo build` 0 warning；`cargo test` 全过；`grep -rn "services/" src/routes/` 仅剩 `probe.rs` + `updater.rs` + `llm_provider.rs`（实质有逻辑的三个）。
- **源**：arch-scout P0-1 / §5.1#1

#### M1-A2 · `AMASEngine` 锁中毒防护
- **维度**：arch
- **估时**：0.5 人日
- **依赖**：M0 完成
- **描述**：`src/amas/engine.rs` 16 处 `lock().unwrap()` / `read().unwrap()` / `write().unwrap()` 统一改成 `lock().unwrap_or_else(PoisonError::into_inner)`（std::sync）或换 `parking_lot::RwLock` / `Mutex`（无中毒概念）。**推荐 parking_lot**（同步引入 `src/state.rs` 中的 `update_cache: Arc<RwLock<...>>`）。同步加 `tests/amas_poison_recovery.rs` 集成测试：故意 panic 持锁 → 后续请求不阻塞。
- **验收**：panic injection 测试通过；`grep "\.unwrap()" src/amas/engine.rs` 仅留正当用法（数组下标 + 已校验 Option）。
- **源**：arch-scout P0-6 / §5.1#5；perf-warden §5.2 P13 注意"保持同步锁，不引入 async 锁"

#### M1-A3 · 删 4 个 stub worker
- **维度**：arch
- **估时**：0.5 人日
- **依赖**：M0-P3（monitoring_aggregate 处置先定）
- **描述**：删 `src/workers/{monitoring_aggregate,etymology_generation,embedding_generation,word_clustering}.rs`；删 `src/workers/mod.rs:225-262` 相关 job 注册；删 `src/config.rs` 对应 `EnableXxxWorker` flag；如有 schema 字段（如 etymology / embedding 列）保留不动（数据库不破坏），仅删代码 + cron。
- **验收**：`cargo build` 0 warning；admin UI 不再显示禁用 worker 卡片；release notes 公告四 worker 退场。
- **源**：arch-scout P0-4 / §5.1#3

#### M1-A4 · sled-migration feature + binary + 依赖一次性清
- **维度**：arch
- **估时**：1.0 人日
- **依赖**：M1-A3 完成（避免并发拆除）
- **描述**：① `src/store/keys.rs` 中被生产代码引用的几个常量（`learning_session_key` 等）内联到 `store/operations/learning_sessions.rs` 等使用方；② 删 `src/bin/migrate_sled_to_sqlite.rs`；③ `Cargo.toml` 移除 `sled = { version = "=0.34.7", optional = true }` + feature `sled-migration` + `[[bin]] migrate-sled-to-sqlite`；④ 删 `src/store/keys.rs`。
- **验收**：`cargo build --all-features` 0 warning；`cargo tree | grep sled` 空；`cargo deny check` 通过。
- **源**：arch-scout P1-4 / §3.2 / §5.1#4

#### M1-A5 · cron scheduler 健康监测
- **维度**：arch / perf
- **估时**：1.5 人日
- **依赖**：M0-P1, M1-A3
- **描述**：① 新表 `worker_last_run`（worker_name PRIMARY KEY, last_run_at INTEGER, last_duration_ms INTEGER, last_error TEXT, last_outcome TEXT）；② `src/workers/mod.rs:add_job` 包装层在 job 完成 / 失败时 upsert；③ admin `/admin/monitoring` 加 "Worker 状态" 区，列每个 worker 最近运行；④ `/metrics`（M0-P1）暴露 `worker_last_run_seconds`；⑤ 调度器层加 health gauge：若任一 worker 连续 3 个调度周期未上报 → admin SSE incident。
- **验收**：admin 控制台可看每个 worker 上一次运行时间；模拟"杀掉 scheduler"事件后 incident SSE 触发。
- **源**：arch-scout P0-3 / §5.2#7；perf-warden §6.2

#### M1-A6 · strict-mode / maintenance 豁免改路由元数据驱动
- **维度**：arch
- **估时**：1.0 人日
- **依赖**：M0 完成
- **描述**：定义 `RouteMetadata { strict_mode_exempt: bool, maintenance_exempt: bool, deprecated_since: Option<...> }`（与 M0-C4 共用 metadata 体系）；`src/middleware/strict_mode.rs:40-47` 与 `src/middleware/maintenance.rs:14-18` 改为读元数据；现在硬编码两份豁免列表不一致的问题彻底消除。
- **验收**：两份硬编码列表不再存在；`tests/middleware_exemption.rs` 集成测试覆盖：admin SSE / `/api/v1/*`（在 410 之前）/ `/status` / `/realtime/events` / `/telemetry` 在各种模式下行为符合元数据声明。
- **源**：arch-scout P1-5 / §5.2#10

#### M1-A7 · 删除 `frontend/lib/queryClient.ts` 死依赖（D10 落实）
- **维度**：arch
- **估时**：0.5 人日
- **依赖**：M0 完成
- **描述**：删 `frontend/src/lib/queryClient.ts`；`frontend/src/main.tsx` 移除 `QueryClientProvider`；`package.json` 移除 `@tanstack/solid-query`；`vite.config.ts` 移除 `vendor-query` chunk；首屏 bundle 减约 30–50 KB。
- **验收**：`npm run build` 0 warning；首屏 raw 总尺寸下降；vitest / playwright 全过；admin 控制台无功能回归。
- **源**：arch-scout P0-5 / §5.1#6；用户拍板 D10

### M1-G · 合规 / 安全

#### M1-G1 · GDPR Article 20 数据导出端点（已拍板 D5）
- **维度**：release / signal
- **估时**：1.5 人日
- **依赖**：M0 完成
- **描述**：新端点 `GET /api/users/me/export`：返回 JSON Lines 格式（决策 O3 选 a），含用户所有可携带数据（profile + records + word_states + favorites + notes + study_config + sessions），按表分块。需要避免长事务，采用 streaming 响应（`axum::response::Response::Stream`）。频率限制：每用户每 24h 1 次。
- **验收**：`tests/gdpr_export.rs` 端到端：创建用户 → 学习 → 收藏 → 导出 → 校验 JSON Lines 完整 → `DELETE /api/users/me` → 重新注册 → 导出为空；导出过程不阻塞其他请求（用 `run_blocking` semaphore）。
- **源**：signal-miner §3.1.6 / §6.2；用户拍板 D5

#### M1-G2 · AMAS LLM 顾问月度硬性成本上限 + 告警（已拍板 D7）
- **维度**：signal / release
- **估时**：1.5 人日
- **依赖**：M0-P1
- **描述**：① migration 加 `llm_advisor_cost_ledger`（month, total_yuan, last_updated_at）；② `src/services/llm_provider.rs` 每次调用后 += 估算成本；③ `Config.llm_advisor_max_cost_per_month` 默认 `100`（决策 O4 选 c + 默认值）；④ 超过上限 → 当月 LLM 顾问 worker 自动 disable + admin SSE incident；⑤ admin UI `/admin/settings` 加月度成本可视化 + 阈值调整。
- **验收**：模拟超上限：next call → `Err(LlmError::MonthlyBudgetExceeded)` + admin 控制台告警可见；下月 1 号自动恢复。
- **源**：signal-miner §3.2.2 / §5.10；用户拍板 D7

#### M1-G3 · feedback_items schema 升级
- **维度**：signal / arch
- **估时**：1.0 人日
- **依赖**：M0 完成
- **描述**：migration `m015` 加 `feedback_items` 字段：`priority TEXT NOT NULL DEFAULT 'normal'`（low/normal/high/urgent）、`status TEXT NOT NULL DEFAULT 'open'`（open/in_progress/resolved/closed）、`assignee_admin_id INTEGER`、`resolved_at INTEGER`、`resolution TEXT`；admin `/admin/feedback` UI 加分类筛选 / 状态切换 / 指派操作；保留向后兼容。
- **验收**：现有 feedback 数据无丢失；admin UI 可分类处理；后端 `POST /api/admin/feedback/:id` 支持更新 status。
- **源**：signal-miner §3.3.3 / §5.8

---

## M2 · 质量门验证（→ v1.0-rc.3）

#### M2-Q1 · k6 压测 5 核心路径
- **维度**：perf
- **估时**：2.0 人日
- **依赖**：M0-P1, M1-G3
- **描述**：在 `tests/load/` 加 k6 脚本（5 个）：登录 / 学习会话开始 / 复习提交 / favorites 列表 / SSE 建连；每路径 ramp 1k → 5k VU × 60s + sustain 60s；输出 P50/95/99 + 错误率，与 `RFC.md §9.1` 目标对比；CI 加 `.github/workflows/load-test.yml` 每周一 03:00 跑（决策 O7 选 a）。
- **验收**：5 路径全过 SLA；不达标的 issue 自动开；脚本入仓且 README 含本地跑法。
- **源**：perf-warden §7 / §9 / §6.1 P1#3

#### M2-Q2 · Lighthouse + Web Vitals 实测
- **维度**：perf
- **估时**：1.0 人日
- **依赖**：M1-A7 完成（首屏 bundle 调整完）
- **描述**：CI 加 `lighthouse-ci.yml`，对 admin 控制台首屏 + `/admin` Dashboard + `/admin/updates` 三个页面跑 Lighthouse；阈值：LCP < 2.5s、TBT < 200ms、CLS < 0.1；Web Vitals 上报采样进 metrics（M0-P1）暴露。
- **验收**：CI 通过；release notes 引用实测数字。
- **源**：perf-warden §3.3 / §7.1

#### M2-Q3 · 客户端契约第四轮 cross-validator（0 P0 / 0 P1）
- **维度**：release
- **估时**：1.0 人日
- **依赖**：M0-C1, M0-C2, M1 全部
- **描述**：M0-C1 是第四轮初版；M2-Q3 是 rc.3 前的最终一次，跑 cross-validator 输出 0 P0 / 0 P1（否则停在 rc.2）。重点验：① M0-C5 410 端点客户端处理 ② M1-G1 导出端点契约 ③ M1-G3 feedback 扩展字段 ④ M1-A7 queryClient 删除后前端没有遗留引用。
- **验收**：`docs/alignment.md` 第四轮终版 0 P0 / 0 P1 / 0 P2 high-impact；merged 到 main。
- **源**：release-keeper §3.1；signal-miner §1 项 10

#### M2-Q4 · v1.0-rc.X 公开后 7 天稳态观测
- **维度**：release / signal / perf
- **估时**：1.0 人日（人工值守）
- **依赖**：M2-Q1, M2-Q2, M2-Q3
- **描述**：rc.3 仅 beta 通道发（决策 O9 选 a）；每日检查（决策 O10 选 b 三源合一）：① GitHub issue tracker 标签 `regression` 无新增 ② admin SSE incident 告警 ③ `/metrics` 暴露的 5xx 错误率 < 0.1%。任一不满足 → 停 GA、修复、重新观测 7 天。
- **验收**：连续 7 天三源全绿 → 触发 GA 流程；释放 v1.0 tag（不带 `-`）。
- **源**：release-keeper §6 GA 门；RFC §6.4

---

## SHOULD（可并行不阻塞 GA）

> 每项均有独立价值，但不阻塞 v1.0 GA。可在 M0/M1/M2 任一阶段穿插开发或推 v1.1。

### S1 · `routes/learning.rs` + `routes/records.rs` 拆分
- **维度**：arch
- **估时**：2.0 人日
- **依赖**：M1 完成
- **描述**：`learning.rs` 1398 行按 lifecycle 拆 `session.rs` / `study.rs` / `progress.rs` / `pick.rs` 4 子文件；`records.rs` 849 行拆 single / batch / sync 三个；保持公开路由签名不变。
- **源**：arch-scout P1-1 / §5.2#8

### S2 · records → AMAS 事件总线化
- **维度**：arch
- **估时**：4.0 人日（v1 内仅文档化承诺，实装推 v1.x）
- **依赖**：S1
- **描述**：把 `process_event` 改成"先 commit 记录、再发事件、AMAS 异步消费"；引入 in-process channel 或 outbox 表。**v1 内仅写 RFC 子文档承诺方向**，实装在 v1.x。
- **源**：arch-scout P0-7 / §5.2#9

### S3 · nginx sample.conf + TLS（certbot）runbook
- **维度**：release
- **估时**：1.0 人日
- **依赖**：M0-D7
- **描述**：`deploy/nginx/wordforge.conf.sample`（含 SSE / gzip / 自更新 long timeout 处理）+ `docs/runbook/nginx-tls.md`（certbot --nginx 一键 + 续期）。
- **源**：release-keeper §4.2 / §6.4 O2

### S4 · maintenance 模式 admin UI 开关
- **维度**：release
- **估时**：0.5 人日
- **依赖**：M0-R4
- **描述**：`/admin/settings` 加 toggle "维护模式"；后端 `POST /api/admin/settings/maintenance` 设置 + SSE 广播；前端 e2e 覆盖。
- **源**：release-keeper §6.4 O4

### S5 · 升级历史审计表 `update_audit_log`
- **维度**：release
- **估时**：0.5 人日
- **依赖**：M0-R3
- **描述**：migration 加 `update_audit_log` 表（admin_id, from_version, to_version, channel, started_at, completed_at, outcome, error）；apply 入口写入；admin UI 加历史列表。
- **源**：release-keeper §6.4 O5

### S6 · ErrorBoundary 接 Sentry / openobserve
- **维度**：arch
- **估时**：1.0 人日
- **依赖**：M0-P1
- **描述**：`frontend/src/components/ErrorBoundary.tsx:14` TODO 实装；可选 Sentry（私有 DSN）或 openobserve（自托管）；至少把异常 stack 上报到 `/api/telemetry/error`。
- **源**：arch-scout P1-6

### S7 · health 端点 `error_rate` 字段实装
- **维度**：arch / perf
- **估时**：0.5 人日
- **依赖**：M0-P1
- **描述**：`src/routes/health.rs:140` TODO 用 M0-P1 metric 替换占位 0。
- **源**：arch-scout P2-1

### S8 · 3 条前端 `it.skip` 复活或删除
- **维度**：arch
- **估时**：1.0 人日
- **依赖**：无
- **描述**：复活 `frontend/tests/pages/admin/UpdatesPage.features.test.tsx:250/275` 两条 polling test（如真不稳，改用 fake timer 或删除并加 issue link）；处置 `AdminLoginPage.features.test.tsx:92` 三次失败计数 skip。
- **源**：arch-scout P1-7 / §3.7

### S9 · `release-calendar.md`
- **维度**：signal / release
- **估时**：0.5 人日
- **依赖**：无
- **描述**：写一份 `docs/release-calendar.md`，登记本仓 / wordforge-web / iOS 三方的发版与兼容窗口；初版只填本仓数据 + 留两个空表给另外两仓自填。
- **源**：signal-miner §5.9

---

## 推 v1.1 / v2 的事项（不在本 backlog 范围）

> 这些事项 RFC §4.3 已声明"v1 不做"。列在这里仅供未来 backlog 参考。

| 项 | 推迟到 | 来源 |
|---|---|---|
| 多实例 / leader 选举 / 集群升级 | v1.1 | release-keeper §6.5 H1 |
| 灰度发布（按版本切流） | v1.1 | release-keeper §6.5 H2 |
| DB 备份外迁（S3 / rsync） | v1.1 | release-keeper §6.4 O3 |
| rate_limit 区分匿名 / 已登录 | v1.1 | perf-warden §6.2 P1#3 |
| SSE 上限提至 5000 + 心跳改 10s | v1.1 | perf-warden §6.2 P1#4 |
| 前端首屏拆 vendor-echarts / vendor-codemirror | v1.1 | perf-warden §6.2 P1#5 |
| `store/operations/extras.rs` 1134 行按主题拆分 | v1.1 | arch-scout P1-3 |
| 14 条 migration down 设计 | v1.1 | arch-scout P1-2 |
| 协作 / 班级 / 多人 | v2 | signal-miner §4 |
| 商业化 / 订阅 / 付费墙 | v2 | signal-miner §4 |
| 切 PostgreSQL / 拆库 | v2 | perf-warden §6.1 决议 |
| AMAS engine actor 化 | v2 | arch-scout §5.3#12 |
| per-user algorithm fine-tune | v2 | signal-miner §4 |
| LSTM / Birdbrain 级自适应 | v2 | signal-miner §4 |
| OAuth 第三方接入 | v2 | signal-miner §4 |
| 用户互助 / 内容共创 | v2 | signal-miner §4 |
| FSRS-style per-word 算法可解释面板 | v2 | signal-miner §3.2.4 |

---

## 附：交叉引用速查

| RFC 章节 | 对应 backlog ID |
|---|---|
| RFC §4.1 MUST 契约 6 项 | M0-C1 ~ M0-C6 |
| RFC §4.1 MUST 发布 4 项 | M0-R1 ~ M0-R4 |
| RFC §4.1 MUST 文档 8 项 | M0-D1 ~ M0-D8 |
| RFC §4.1 MUST 架构 7 项 | M1-A1 ~ M1-A7 |
| RFC §4.1 MUST 性能 5 项 | M0-P1 ~ M0-P5 |
| RFC §4.1 MUST 合规 3 项 | M1-G1 ~ M1-G3 |
| RFC §4.1 MUST 质量门 4 项 | M2-Q1 ~ M2-Q4 |
| RFC §4.2 SHOULD 9 项 | S1 ~ S9 |
| RFC §4.3 WON'T 10 项 | 见本文末尾"推 v1.1 / v2"表 |
| RFC §7 风险 R01 ~ R36 | 每条已注关联任务 |
