# Changelog

所有版本变更记录均在此文件。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

后续每次发版在文件顶部 append 最新条目；全量重建可运行 `bash scripts/build-changelog.sh`。

---

## [Unreleased] — v1.1.0-rc.1（资源包热更后端）

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
