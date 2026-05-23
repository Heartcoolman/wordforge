# Changelog

所有版本变更记录均在此文件。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

后续每次发版在文件顶部 append 最新条目；全量重建可运行 `bash scripts/build-changelog.sh`。

---

## [v1.1.0-beta.2] — 2026-05-23 · Pre-release · 自更新诊断盲区修复

v1.0.0 生产服务器（8.135.57.148）执行 admin 一键升级到 v1.1.0-beta.1 失败，60s /health 检查未通过 → 自动回滚到 v1.0.0。诊断时发现：`src/services/updater.rs::spawn_replacement` 把新进程 stdout/stderr **全部 Stdio::null() 丢弃**，导致新进程的 panic / migrate 失败 / bind 失败等任何 stderr 输出在 systemd journal 里**一行都看不到** —— 父进程探针 60s 超时是唯一可见信号，根因完全盲猜。

本 release **不修升级失败本身**（真根因还没拿到证据，等本次升级跑完拿到日志再说），只修**让下次升级能拿到 stderr** 的诊断盲区，双管齐下：

### 修复

- 🔧 **`src/main.rs` 加 `redirect_self_update_logs_if_applicable`**（A 修，本次升级救场）：main 函数第一行（dotenvy / Config::from_env 之前）就检测 `/proc/<ppid>/cmdline` 是否含 `wordforge-restart` 字符串（updater sh wrapper 的固定 argv[1]）。如果是 → `dup2` stdout/stderr 到 `<install_dir>/logs/updater-child-<unix_ts>.log`。否则不动 stderr（systemd journal 继续接管）
    - 关键：必须在**任何可能 panic 的代码**之前跑，因此放在 main 第一行
    - 时间戳放文件名内，多次升级日志不互相覆盖
- 🔧 **`src/services/updater.rs::spawn_replacement` 改 Stdio::null() → logged file**（B 修，未来升级安全）：把子进程 stdout/stderr 改为 redirect 到 `<install_dir>/logs/updater-child.log`（append 模式）；开不了日志文件时回退 null + tracing warn

### 为什么 A + B 双修

| 修复 | 解决的场景 |
|---|---|
| A（main.rs） | **本次** v1.0 → beta.2 升级：v1.0 二进制跑的是 v1.0 编译时的旧 spawn_replacement（Stdio::null 写死在二进制里，没法回头改），但新 beta.2 二进制启动后立刻自己 dup2 救场 |
| B（updater.rs） | **未来**升级（beta.2 → 后续版本）：父进程是 beta.2，直接给子进程开日志 fd |

### 这次升级失败的真根因（pending）

beta.2 发布 + 生产再次触发 admin 一键升级后，看 `/opt/wordforge/logs/updater-child-*.log` 的真 stderr 才能定位。当前**几个高概率假设**（按可能性排序，等数据来验证）：

1. **migration m020 / m021 在 v1.0 prod db 上跑失败**（m021 已确认幂等，m020 是 4 张新表 CREATE IF NOT EXISTS 看似 OK，但仍可能某条 CHECK constraint 与现有数据冲突）
2. **新进程 bind 端口失败**（旧进程 sock 没 release / SO_REUSEADDR 没设）
3. **新引入的 startup 依赖**（如资源包签名器初始化时 key dir 权限）
4. **dotenvy 读 .env 在新进程的 cwd 下找不到**（systemd EnvironmentFile 注入到父，子继承应该 OK，但需验证）

### 版本号

- `Cargo.toml` + `Cargo.lock`: `1.1.0-beta.1` → `1.1.0-beta.2`

---

## [v1.1.0-beta.1] — 2026-05-23 · Pre-release

v1.1 首发 Pre-release。涵盖 P0（资源包热更 + GDPR 真流式 NDJSON）、P1（领域事件总线）、
P2（重构 / 性能 / 文档 / clippy 清零）三阶段全部工作。

**版本号补齐说明**：v1.0.0 / v1.0.0-rc.1 / v1.0.0-rc.2 三次发版**漏改了 `Cargo.toml` 的
`version` 字段**，导致这三个 tag 的二进制内嵌版本号实际是 `0.6.0-beta.4`（git tag 名对、
binary `CARGO_PKG_VERSION` 错）。证据：`git show v1.0.0:Cargo.toml | grep ^version` 返回
`0.6.0-beta.4`。生产影响极小：代码内仅 `src/state.rs:401` 一处用 `env!("CARGO_PKG_VERSION")`
作 fallback；admin 一键升级 caller 几乎总会显式传 version，fallback 路径基本不触发。
本 release 把 `Cargo.toml` + `Cargo.lock` 一并补齐到 `1.1.0-beta.1`，跨越两个失同步 release。

### Release 流程调整 · rc 通道废弃（follow-up）

v1.1.0-beta.1 发布后决定**只走 beta + GA 两条线**，rc 通道废弃。改动：

- 🔧 **`.github/workflows/release.yml` tag-lint 收紧**：原 `^v\d+\.\d+\.\d+-(alpha|beta|rc)\.\d+$` → `^v\d+\.\d+\.\d+-beta\.\d+$`。`v1.2.0-rc.1` / `v1.2.0-alpha.1` 这类 tag 投递会被 CI 拦下
- 🗑️ **删除 `scripts/rc-observation/` 整套 7 天稳态观察脚手架**（5xx 收集 / SSE incident / GH regression / daily report 共 4 脚本 + README）+ 配套测试 `tests/rc_observation_scripts.rs`（299 行）+ runbook `docs/runbook/rc-observation-{thresholds,report}.md`。GA 门改为由 beta 内测充分性 + 既有质量门（clippy/test/audit）保证，不再需要 rc 阶段独立的 7 天三源观察期
- 🔧 **`scripts/ga-regression-check.sh` 移除 M2-Q4 段 + § 6.4 GA 门里的 7 天日报判定段 + 5xx/阈值文档 check**（共 3 段），保留 S5 自更新审计 check
- 📚 **`docs/release-calendar.md`** 流程表去掉 Release Candidate 行；v1.1 计划从"三段 rc 合并切 GA"改为"beta 单通道，beta.1 已发 → 内测无 P0 后切 GA"
- **历史保留**：v1.0.0-rc.1 / v1.0.0-rc.2 git tag + GitHub Pre-release 不动（v1.0 发版过程的真实历史）；`docs/alignment.md` / `docs/changelog.md` 内 v1.0 历史段的 rc 引用不动

### P0 收尾（本 release 新增）

- 🚀 **GDPR 导出真流式 NDJSON**（`src/routes/users.rs`）：`/api/users/me/export` 改用
  `tokio::sync::mpsc` + `axum::body::Body::from_stream`，每个 store 任务读完一块即推入
  channel；HTTP/1.1 自动 `Transfer-Encoding: chunked`。修复原 `Vec.join` 一次性 body
  在大用户（几十 MB records）下的内存膨胀，客户端可边读边写盘。
    - 错误处理取舍：流中段 store 任务失败时，向 channel 推一行 `{"table":"_error",...}`
      后关闭，**不**回退为 HTTP 5xx（status code 在首块 flush 时已定，无法回退） ——
      流式 API 的固有约束，已在源码注释明示
- 📝 **admin auth 注释补完**（`src/auth.rs`）：明示当前 admin session 校验**不做**用户表
  跨查（之前实现把 `admin_id` 当 `user_id` 去 `users` 表查 `is_banned` 会把所有正常 admin
  拒之门外）；管理员"禁用"目前靠 `locked_until` + `delete_admin_session` 两条机制；
  真正的 admin `is_disabled` 列需 schema 迁移，超出 v1.1 P0 范围

### 质量（原 rc.3 阶段）

- ✅ **cargo clippy --all-targets -- -D warnings 零警告零错误**：56 条历史警告
  按「真问题修复 / 习语局部豁免 + 注释」两条线清零（P2.1）
    - **真修复**：`tests/llm_cost_ledger.rs` 3.14 → 1.23（误中 approx_constant PI 阈值）；
      `tests/admin_analytics_seeded_http.rs:327` 多余 `&` borrow 去除；
    - **局部豁免（带理由注释）**：
      `src/amas/engine.rs` / `src/amas/memory/ssp.rs` / `src/amas/monitoring.rs` /
      `src/store/operations/elo.rs` / `src/store/operations/system_settings.rs` /
      `tests/amas_param_sweep.rs` / `tests/property_memory_models.rs`
      各自 `mod tests` 或 crate-level `#[allow(clippy::field_reassign_with_default)]`
      —— 测试用 `let mut cfg = X::default(); cfg.field = v` 比 struct-update 语法
      更易看清「这条 case 改了什么」，是公认习语；
      `src/workers/error_rate_watchdog.rs:tests` 串行化全局静态用 std::sync::Mutex
      跨 await，#[tokio::test] 单线程不死锁，`#[allow(clippy::await_holding_lock)]`；
      `src/store/operations/resource_packs.rs::ResourcePackChannel::from_str`
      故意返回 `Option<Self>` 而非 `Result<Self,Err>`，标 `#[allow(clippy::should_implement_trait)]`；
      `src/store/operations/amas_telemetry.rs::tests::insert_event` 8 参数 helper
      `#[allow(clippy::too_many_arguments)]`。
- ✅ **cargo test 全过**：lib 641 + 各集成测试 binary 累计 **921 passed / 0 failed**
  （lib 单测 641 + tests/* 各 binary 合计 280；cargo test 按 binary 分组报，去重后总数 921）

### v1.1.0 全 RC 汇总（P0 + P1 + P2 = 25 commit）

| 阶段 | 范围 | commit 数 | 状态 |
|---|---|---|---|
| **P0** | 资源包热更后端（rc.1 落地） | 12 | ✅ |
| **P1** | 领域事件总线基础设施（rc.2） | 1 | ✅ |
| **P2** | 重构/性能/文档/收尾（rc.2 + rc.3，含 P2.1 clippy 清零） | 12 | ✅ |

### Release Notes（面向用户）

v1.1.0 GA 给 iOS / Web 客户端的核心交付：
- **资源包热更**：词书内容更新无需发版（详见 rc.1 段）
- **事件总线**：records → AMAS 通过领域事件解耦，未来扩展更解耦（开发者体验）
- **运维就绪**：维护模式 UI 开关 + nginx 反向代理样例 + TLS runbook + Sentry 错误监控
- **稳定性**：rate_limit 双轨防恶意匿名爬取、SSE 上限提升 5× 适配大客户、21 条迁移可逆便于 dev 重置
- **质量门**：clippy --deny 零警告 + 921 测试 100% 通过

### 过程节点 · rc.2（事件总线 + 重构 + 文档）

rc.1（资源包热更）落地后，本 RC 交付 P1 + P2 主体工作。

### 新功能（事件总线）

- 🚀 **领域事件总线**（P1-S2，commit 568f294）：records 写入旁路 emit `DomainEvent::LearningRecorded` →
  AMAS engine 异步消费，事件总线（`src/services/event_bus.rs`）单向广播 + per-receiver
  缓冲，避免 records→AMAS 紧耦合直接调用。新增 `tests/event_bus.rs` 单测覆盖 emit/recv
  与背压 drop 行为。

### 改进（rate_limit）

- 🛡️ **rate_limit 双轨**（P2.3，commit fb9f5c1）：匿名按 IP 限流，已登录按 `user_id` 限流。
  防止单个恶意未登录扫描器拖累全站，同时已登录用户在多设备/移动网络下不再被 IP 误伤。
  `RateLimitConfig` 加 `anonymous_max_requests` / `authenticated_max_requests` 两挡。
- ⚡ **SSE 上限放宽**（P2.4，commit 53c4294）：每用户最多 1000 → **5000** 路活跃连接，
  心跳 15s → **10s**。给同时挂 10 端的「大用户」预留余量。

### 改进（迁移 / 拆分 / 审计）

- 🔧 **21 条 migration down 设计**（P2.2，commit 4d342c3）：m001–m021 全部配套 down 函数 +
  `revert_to(target_version)` 接口，**生产严禁调用**（注释明示），仅供 dev / test 重置脏
  schema。`tests/migrate_down.rs` 集成测试验证 up → down → up 循环幂等。
- 🔧 **store/operations/extras.rs 按职责拆 3 块**（P2.5，commit 3cce60b）：937 行单文件
  拆为 `user_profile_extras.rs` / `word_metadata_extras.rs` + 保留少量 extras 杂项。
  `mod.rs` 同步导出，外部 API 兼容。
- 🛡️ **`update_audit_log` 通用化**（P2.10，commit e37cb69）：迁移 021 给表加
  `action / target_type / target_id / metadata_json` 4 列。覆盖 7 处 admin 敏感
  handler：资源包 upload/set_active/deactivate + 用户 ban/unban/reset_password/set_password。
  老 self_update 写入路径完全兼容，新写入通过 `Store::insert_admin_audit()` 通用入口。
  `GET /api/admin/updates/history` 返回 entry 新增 4 个 camelCase 字段。

### 数据库迁移

- `021_admin_audit_log_v2`：ALTER TABLE 加 4 列（含 `action TEXT NOT NULL DEFAULT 'self_update'`），
  老行回填默认值，向后兼容

### 文档（运维 / 监控）

- 📚 **release-calendar.md 补 v1.0 GA + v1.1 计划**（P2.7，commit 5b82688）
- 📚 **nginx sample.conf + TLS runbook**（P2.8，commit 3fb7460）：反向代理样例 + Let's Encrypt
- 📚 **维护模式运维 SOP runbook**（P2.9，commit 666ac5e）
- 🛡️ **前端 ErrorBoundary 接 Sentry SDK**（P2.6，commit 3817047）

### 测试

- 新增 `tests/event_bus.rs` 单元测试覆盖事件总线 emit/recv/背压
- 新增 `tests/migrate_down.rs` 集成测试覆盖 up→down→up 循环
- `tests/admin_extra_http.rs` 加 `it_admin_sensitive_actions_write_audit_log`
- `tests/resource_pack_http.rs` 加 `admin_resource_pack_handlers_write_audit_log`

### 过程节点 · rc.1（资源包热更后端）

iOS v1.1 客户端 POC 已落地（`/Users/liji/WordForge-App` @ `b1e1a41`），本 RC 交付后端配套，
路径协议详见 [`docs/backend-handoff-resource-pack-v1.1.md`](docs/backend-handoff-resource-pack-v1.1.md)
和 [`docs/v1-research/v1-1-resource-pack.md`](docs/v1-research/v1-1-resource-pack.md)。

### 新功能（资源包子系统）

- 📦 **资源包匿名端点**：`GET /api/resource-packs`、`GET /api/resource-packs/:packId/manifest`、`GET /api/resource-packs/public-key`（详见 `docs/api-endpoints.md` §21）
- 📦 **CDN 自托管**：`static/packs/<pack>/<ver>/payload.json` 走 ServeDir + `public, max-age=31536000, immutable`
- 📦 **Admin CRUD**：`POST/PUT/GET/DELETE /api/admin/resource-packs/*`（详见 §22），含 multipart 上传后自动 SHA256 + Ed25519 签名 + 落盘 + 写表
- 📦 **客户端 telemetry**：`POST /api/telemetry/resource-pack-install` 落 `resource_pack_install_log` 表
- 🛡️ **Ed25519 签名链路**：`ed25519-dalek v2`，私钥 0600 / 公钥 0644 自动 bootstrap，与既有 minisign 自更新链路解耦
- 🛡️ **SSE 事件 `resource_pack_available`**：admin 切 active 后 5 分钟 dedup 广播
- 📊 **install_log 聚合**：admin `/stats` 端点按 `(version, outcome)` 计数

### 数据库

- 迁移 `020_resource_packs`：4 张表 `resource_packs` / `resource_pack_versions` / `resource_pack_active` / `resource_pack_install_log`，channel CHECK 限定 `stable/beta/internal`。down 迁移随 P2.2 统一补

### 错误码

- `RESOURCE_PACK_NOT_FOUND` (404)、`RESOURCE_PACK_APP_VERSION_TOO_LOW` (409)、`RESOURCE_PACK_CHANNEL_FORBIDDEN` (403)、`RESOURCE_PACK_SIGNER_UNAVAILABLE` (503)

### 修复

- 🐛 **`sse_event_table` 测试假性平衡**：v1.0 M1-A5 `worker_missed` 和 M1-G2 `llm_budget_exceeded` 未同步进 `DOCUMENTED_EVENTS` 和 `all_sse_event_samples()`，测试 11==11 巧合通过；本 RC 补齐至 14 个变体（含新 `resource_pack_available`）

### 实施时的 plan 纠正

- ❎ Plan 原写「扩展 `services::updater::Channel` 加 Internal 变体」，发现那是面向二进制自更新的 release 通道（牵连 checker / cache / apply 整套），改为**独立** `ResourcePackChannel { Stable, Beta, Internal }`（`store::operations::resource_packs`），业务语义分离更干净
- ❎ 集成测试发现 `downloadURL` 全大写 URL 在 serde camelCase 下变成 `downloadUrl`（U 小写），必须手动 `#[serde(rename = "downloadURL")]`

### 与 iOS 客户端的协调点

iOS v1.1 客户端文档原写 `GET /api/v1/resource-packs/{packId}/manifest`，但后端 `/api/v1/*` 自 2026-05-21 起整组冻结返回 410 Gone（sunset 2027-01-01）。后端实现走 `/api/resource-packs/*` 主端点（与 `/api/wordbooks/*` 风格对齐），**iOS 需改 `EndpointServices.swift:519` 的 path + 同步对接文档**。

### 测试

- 9 个新增集成测试 `tests/resource_pack_http.rs`：upload→activate→manifest 端到端、ETag/Cache-Control/304/409/404、telemetry 聚合、SSE dedup
- 全测试 635 lib + 130+ 集成测试 0 失败

---

## [v1.0.0] — 2026-05-22 · GA 🎉

**51 项 v1 工作全部完成**（MUST 37 + SHOULD 9 + 新增 5），76 commit 跨两个 RC 通道。完整 release notes 见 GitHub Release v1.0.0。

### 重大变化（Breaking）

- 🚨 **`/api/v1/*` 全部端点返回 410 Gone**（M0-C5）：v1 路由刻意绕过 AMAS 的设计警告，老客户端必须迁移至 `/api/learning/*` / `/api/records/*`
- 🚨 **WordState wire 序列化改为 lowercase**（M0-C1）：第四轮 cross-validator P1-W1 对齐
- 🚨 **删除 `sled-migration` feature**（M1-A4）：已无 sled→sqlite 迁移需求

### 新功能

- 🛡️ **minisign 签名链**（M0-R2）：release tarball 全签名，binary 内嵌公钥防 downgrade attack。公钥见 [`docs/security/wordforge-release.pub`](docs/security/wordforge-release.pub)
- 🛡️ **GDPR 数据导出**（M1-G1）：`/api/users/me/export` JSON Lines 全量导出
- 🛡️ **`update_audit_log`**（S5）：升级链全程审计可追溯
- ⚡ **`/metrics` Prometheus 端点**（M0-P1）：HTTP 计数器 / `worker_last_run` / histogram
- ⚡ **`error_rate_watchdog`**（M0-P4）：5xx 滚动告警 SSE incident
- ⚡ **k6 + Lighthouse CI**（M2-Q1/Q2）：周一 03:00 自动跑
- 💰 **LLM 月度成本上限**（M1-G2）：admin 后台可调，默认 ¥100，`SseEvent::LlmBudgetExceeded` 提醒
- 📝 **8 篇文档全新入仓**：README + SECURITY + CONTRIBUTING + auto-update + 5 篇 runbook + 4 篇 user docs + word-states

### 改进

- 🧹 **删 services 层**（M1-A1）：handler 直接依赖 `Store` + `AMASEngine`
- 🧹 **删 4 个 stub worker**（M1-A3）：`monitoring_aggregate` / `etymology_generation` / `embedding_generation` / `word_clustering` 自引入起从未启用
- 🧹 **删 `@tanstack/solid-query` 死依赖**（M1-A7）：前端改用 `createResource` cache map
- 🔧 **strict-mode / maintenance 改路由元数据驱动**（M1-A6）
- 🔧 **AMASEngine `parking_lot` 锁中毒防护**（M1-A2）+ `amas_poison_recovery` 集成测试
- 🔧 **cron scheduler 健康监测**（M1-A5）：migration + SSE `WorkerMissed`
- 🔧 **`routes/learning.rs` + `records.rs` 按 lifecycle 拆分**（GA bonus，commit 1c8d27f）

### 已知遗留（推 v1.1）

- clippy 56 历史警告（M0 / M1 都接受了，v1.1 集中清）
- S2 events 总线化（v1.0 文档化承诺，见 `docs/v1-research/should-deferred.md`）
- 真实 7 天稳态观测（脚手架就位，rc.3 公开后用户自跑 `scripts/rc-observation/`）

### 测试门

- ✅ §6.1（M0 → rc.1）：824 cargo / 926 vitest / 0 P0 alignment
- ✅ §6.2（M1 → rc.2）：873 cargo / 925 vitest
- ✅ §6.3（M2 + SHOULD → GA）：873 cargo / 925 vitest / k6 + Lighthouse 入仓
- 🟡 §6.4：7 天观测脚手架就绪

---

## [v1.0.0-rc.2] — 2026-05-22 · Pre-release · M1 + M2 + 6 SHOULD（24 项）

### 新功能

- **M1-G1** GDPR JSON Lines 数据导出
- **M1-G2** LLM 顾问月度人民币成本上限（默认 ¥100）
- **M1-G3** feedback_items 升级 priority / status / assignee / resolution
- **M1-A5** cron scheduler 健康监测 + SSE `WorkerMissed`
- **M2-Q1** k6 5 路径压测脚本 + load-test.yml CI
- **M2-Q2** Lighthouse CI + Web Vitals
- **M2-Q4** rc-observation 4 脚本 + 2 runbook + 集成测试
- **S3** nginx + TLS 部署 runbook（certbot 流程）
- **S4** admin 维护模式即时切换
- **S5** `update_audit_log` 表 + 审计追踪
- **S6** ErrorBoundary 接入 `/api/telemetry/error`
- **S7** `health.error_rate` 字段实装
- **S9** `release-calendar.md` 三仓发版日历

### 改进

- **M1-A0/A0a/A0b** clippy 清债（M0 新增警告全清，56 历史警告留 v1.1）
- **M1-A1** 删 `LearningService` / `WordbookService` / `AdminService`
- **M1-A2** AMASEngine 锁中毒防护
- **M1-A6** strict-mode / maintenance 改路由分组驱动
- **M1-A7** 删 `@tanstack/solid-query` 死依赖
- **M2-Q3** alignment.md 第四轮终版
- **S8** 复活 AdminLoginPage 3 个失败测试

### 移除

- **M1-A3** 删除 4 个 stub worker（`monitoring_aggregate` / `etymology_generation` / `embedding_generation` / `word_clustering`，自引入起 `enabled: false`）
- **M1-A4** 删除 `sled-migration` feature + binary + 依赖

---

## [v1.0.0-rc.1] — 2026-05-22 · Pre-release · M0 基础修复 + 安全网（23 项）

### 新功能

- **M0-C2** OpenAPI 3.1 集中声明 + utoipa 25 端点 + CI drift 防漂
- **M0-R1** `release.yml` pre-release-tag-lint step：tag 含 `-` → Pre-release，否则 → Latest
- **M0-R2** minisign 签名链 + 防 downgrade attack（编译期嵌入公钥）
- **M0-R3** 自更新回滚增强 + systemd unit 入仓
- **M0-R4** apply swapping 自动维护模式
- **M0-P1** `/metrics` Prometheus 端点 + HTTP 计数器 + worker_last_run + histogram
- **M0-P3** monitoring_events retention + 月度 VACUUM
- **M0-P4** `error_rate_watchdog` 5xx 滚动错误率告警
- **M0-P5** apply 各 phase 独立 watchdog 超时 5 分钟
- **M0-D1..D9** 8+1 篇全新文档：README + SECURITY + CONTRIBUTING + auto-update + 5 篇 runbook + 4 篇 user docs + word-states

### 重大变化

- **M0-C1** WordState wire 序列化改为 lowercase（cross-validator P1-W1）
- **M0-C5** `/api/v1/*` 全部端点返回 410 Gone

### 改进

- **M0-C3** SSE 事件表补齐 5 条缺失变体
- **M0-C6** `verify-*-auto-update*.sh` 双通道契约修复
- **M0-D6a** VitePress srcExclude 排除内部目录
- **M0-P2** `.env.example` 对齐 + `verify-env-example.sh`

---

## [v0.6.0-beta.4] — 2026-05-20 · Pre-release

### 改进

- `admin/updates`：Release Notes 渲染 markdown（##/列表/code fence/inline）
  - 新增 `ReleaseNotesMarkdown.tsx` 组件，支持标题 / 无序有序列表 / 围栏代码 / bold / inline code / 链接
  - 链接只允许 http/https，零 XSS 风险
  - 11 个单测覆盖所有 pattern + 边界（空内容 / 纯文本 / 恶意 javascript: 协议）
  - `UpdateChannelCard.tsx` 把 `<pre>` 换成 `<ReleaseNotesMarkdown />`；max-height 扩至 h-80

---

## [v0.6.0-beta.3] — 2026-05-20 · Pre-release

### 新功能

- **admin/updates 双通道**：Stable 主卡 + Beta 折叠区，后端单 URL `/releases?per_page=10` 分流两个通道
  - `Channel` enum / `ChannelStatus` 新结构；`updater.rs` 双通道 parse / cache / apply
  - 前端 `UpdateChannelCard.tsx`（可复用单通道卡片）+ `Collapsible.tsx`（通用折叠条 + ARIA）
  - `UpdatesPage.tsx` 全量重写：顶部当前版本 + 立即检查；主区 Stable；下方 Beta 折叠区 + badge
- **release.yml 显式 prerelease 规则**：tag 含 `-` → Pre-release，否则 → Latest stable

### 修复

- `UPDATE_CHECK_API_URL` 默认改为 `/releases?per_page=10`（禁 hardcode `/tags/...`）

---

## [v0.6.0-beta.2] — 2026-05-20 · Pre-release

### 修复

- `admin/feedback`：修复首次进入即 ErrorBoundary 全屏崩溃
  - 根因：`listFeedback` 类型签名写成 `{ items }` 而后端 `paginated()` 实际返回 `{ data }`
  - 修复：`admin.ts` 类型签名 `items→data`；`FeedbackPage.tsx` 改用 `resp.data`；加 2 个契约测试锁定字段

---

## [v0.6.0-beta.1] — 2026-05-20 · Pre-release

### 新功能

- **远程 REPL Probe**：M4–M6 全量上线（broadcast / 限速 / 历史查询 / 清理 cron / kill switch / CodeMirror UI / 模板 + 历史回放 / 集成测试）
- **UI 全量加固**：基础组件 21 个 a11y/loading/motion-reduce 统一；admin 顶层页面 15 个 KPI 错误降级 / stagger / SSE；AMAS 9 个子面板参数编辑 / 预设 / JSON 交互统一；业务 / auth / layout 8 个边界完善
- **静态审计 P0/P1/P2 全量修复**

---

## [v0.5.6] — 2026-05-19

### 修复

- `updater`：GitHub release 镜像 prefix + 拆 download client read_timeout（解决国内网络超时）

---

## [v0.5.5] — 2026-05-19

_版本号 bump，包含 v0.5.4 修复的稳定化产物。_

---

## [v0.5.4] — 2026-05-19

### 修复

- `updater`：镜像 prefix 支持 + download client 拆独立 read_timeout

---

## [v0.5.3] — 2026-05-19

### 修复

- `updates`：`apply` 异步化 + strict-mode 豁免 SSE/status 端点（解决长流程自更新被中间件误拦）

---

## [v0.5.2] — 2026-05-19

### 修复

- `updates`：apply 异步化防 handler 长跑同步 await 阻塞 Tokio worker

---

## [v0.5.1] — 2026-05-19

### 修复

- `updater`：补 v0.4.4 dangling tag 的端口重试绑定修复

---

## [v0.5.0] — 2026-05-19

_admin 一键升级七连发起点，整合 v0.4.4 端口竞态修复与完整自更新链路。_

---

## [v0.4.4] — 2026-05-18

### 修复

- `updater`：避免自更新期间端口竞态（port race during self update）

---

## [v0.4.3] — 2026-05-18

### 修复

- `updater`：Codex review 第二轮两条意见修复
- E2E：修复 24 个失败用例，移除 `continue-on-error`

---

## [v0.4.2] — 2026-05-17

### 改进

- `workers`：cron day-of-week 使用 SUN 字符串修复
- `store`：`init_schema` 仅在全新 DB 跑全量 DDL

---

## [v0.4.1] — 2026-05-17

### 改进

- AMAS 调参：`memoryModel` 11 维 Tier-A 调参（+10.6% prediction / +14% memory / −25% ICI）
- `learning / word-states`：支持 audio / spelling 模式 + due-review ETA
- `auth`：`verify-reset-token` 对无效 / 过期 token 返回 200 `{valid:false}`

---

## [v0.4.0] — 2026-05-02

### 新功能

- **admin AMAS 调参后台产品化**：结构化编辑 + 可视化 + DeepSeek 助手
- **admin 自更新（GitHub Releases · 仅 Linux）**：后台自动检查 + 一键触发
- 文档站基础介绍补齐

---

## [v0.3.4] — 2026-04-29

_补丁版本，含稳定性修复。_

---

## [v0.3.3] — 2026-04-28

_补丁版本，含稳定性修复。_

---

## [v0.3.2] — 2026-04-25

_补丁版本，含稳定性修复。_

---

## [v0.3.1] — 2026-04-24

### 新功能

- `release.yml`：tag 触发，自动编译 Linux x86_64/aarch64 静态二进制并上传到 GitHub Release
- `install.sh`：服务器一键安装/升级，自动生成 JWT 密钥、创建 systemd 服务

---

## [v0.3.0] — 2026-04-17

### 新功能

- **async runtime 防阻塞改造**：全量同步 I/O 迁入 `run_blocking` 线程池，避免阻塞 Tokio worker

---

## [v0.2.9] — 2026-04-15

_补丁版本，含稳定性修复。_

---

## [v0.2.8] — 2026-04-14

### 新功能

- 遥测增强
- 客户端管理
- 心跳看门狗（heartbeat watchdog）

---

## [v0.2.7] — 2026-04-11

### 修复

- SSE 重连：401/403 不再永久退出，改为以最大延迟重连，登录后自动恢复
- 遥测 worker：无 token 时重置窗口数据，避免跨会话污染

---

## [v0.2.6] — 2026-04-10

### 新功能

- 服务端选词接口
- OpenAPI 规范页面
- 文档完善

---

## [v0.2.5] — 2026-04-09

### 改进

- 存储层重构
- AMAS 增强
- 全栈精简优化

---

## [v0.2.0] — 2026-03-24

### 新功能

- **AMAS v2 智能记忆引擎**：全新 DSR 架构，多模型记忆曲线 + ELO + 疲劳衰减

---

## [v0.1.4] — 2026-02-15

### 改进

- 性能优化
- AMAS 准确度提升
- 代码质量治理
- 全链路测试修复

---

## [v0.1.3] — 2026-02-14

### 新功能

- 版本更新检查
- 全局中文化

---

## [v0.1.2] — 2026-02-14

### 修复

- 审查修复
- 管理员密码重置
- 测试覆盖率提升
- README 完善

---

## [v0.1.1] — 2026-02-14

### 改进

- 前端功能完善：审计修复 + 多模块功能增强

---

## [v0.1.0] — 2026-02-12

首次发布。包含完整的 Rust 后端 + SolidJS 管理后台 + AMAS 自适应学习引擎。
