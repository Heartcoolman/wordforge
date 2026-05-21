# WordForge v1.0 RFC

> 状态：draft
> 起草日期：2026-05-21
> 团队产物：本文件由 wordforge-v1-research 团队汇总自 01–04 调研报告
> 当前版本：`v0.6.0-beta.4`（main HEAD `d0325f8`，最近发版 `v0.6.0-beta.3`）
> 范围声明：**本仓 v1.0 = 后端 + admin + API SDK v1.0**；学习端（wordforge-web）与移动端（iOS）各自独立发版。

---

## 1. 背景

WordForge 已交付 v0.5.0 → v0.6.0-beta.3 的"七连发"：admin 一键升级双通道闭环、客户端×后端契约三轮 100% 对齐、AMAS 调参 +10.6% prediction / +14% memory / −25% ICI。在 beta.4 工作树上启动 v1.0 立项调研，四个并行维度（架构 / 反馈竞品 / 性能 / 发布契约）已完成产物归档：

- `docs/v1-research/01-arch-scout.md`（27.7 KB · 模块清单 + P0–P2 技术债 + gap matrix + 升级候选）
- `docs/v1-research/02-signal-miner.md`（27.0 KB · 内部 TOP 10 + 5 家竞品矩阵 + 必备清单 + 风险 10 条）
- `docs/v1-research/03-perf-warden.md`（27.6 KB · benchmark / 配置 / SLO / 容量 / 稳定性 + SLA 提案）
- `docs/v1-research/04-release-keeper.md`（31.4 KB · 发布流 / admin/updates / 契约稳定性 / 文档矩阵 / Runbook + 收口 26 项）

四份报告交叉验证一致点高，仅在"路由级监控 ↔ Prometheus 端点"、"`.env.example` 一致性 ↔ 发布前自检"两处存在"同一件事两个视角"，已在本文 §7 风险登记册合并去重。

---

## 2. 范围声明（最重要的边界）

> **本仓 v1.0 ≠ WordForge 产品 v1.0**。

本仓 v1.0 承诺三件事：

1. **后端运行时 1.0**：单实例 axum + SQLite + AMAS 引擎；接口兼容性进入 v1 stable 契约。
2. **admin 控制台 1.0**：Solid SPA 内置在二进制；管理员端 UX 与运维链路稳态化。
3. **API SDK v1.0**：60+ REST 端点 + 9 个 SSE 事件分入 `v1 stable / v1beta / v0-internal` 三档（详 §9 与 `04-release-keeper.md` §3.2）。

**不在本仓 v1 范围**：

- 学习端（wordforge-web 独立仓库的 SPA）
- 移动端（iOS / Android 客户端）
- 协作 / 班级 / 多人功能（推 v2，signal-miner §4）
- 商业化 / 订阅 / 付费墙（推 v2）
- per-user fine-tune / LSTM 自适应（推 v2）
- 多实例 / 集群 / HA（推 v1.1，perf-warden §5.2 P3 / release-keeper §6.5）
- 切 Postgres / 拆库（推 v2，perf-warden §6.1 决议）

---

## 3. v1.0 三大目标

### 3.1 稳定性收口（GA）

- 路由级延迟 / 错误率监控**首次上线**（无监控是当前最大盲区）。
- 9 项已发生回归 + 15 项潜在风险（perf-warden §5）全部进风险登记册并关闭。
- 算法基线已实测可拍板：AMAS `nearMiss[0]` 配置直接采纳为 GA 默认。

### 3.2 半成品 / 临时绕过补齐

- `/api/v1/*` 立即 410 Gone（已拍板，§5）。
- 4 个 stub worker（MonitoringAggregate / EtymologyGeneration / EmbeddingGeneration / WordClustering）删除。
- 3 个 services 空壳层做减法（删 LearningService / WordbookService，AdminService 整合）。
- AMASEngine 16 处 `lock().unwrap()` 锁中毒防护。
- sled-migration feature + binary + 依赖一次性清。
- feedback_items 表 schema 扩展（priority / status / assignee / resolved_at）。
- monitoring_aggregate worker 实装或加 retention。

### 3.3 架构升级（在 1.0 边界做的不可逆调整）

- API 契约稳定性级别正式生效（v1 stable / v1beta / v0-internal 三档 + Deprecation/Sunset header）。
- OpenAPI 自动生成（utoipa 注解链 + CI diff 防漂移）（已拍板 v1 必做）。
- Prometheus exporter 上线（`/metrics` 端点）。
- minisign / cosign 二进制签名（已拍板 v1 必做）。
- GDPR Article 20 数据导出端点（已拍板 v1 必做）。
- AMAS LLM 顾问月度成本硬上限 + 告警（已拍板 v1 必做）。
- strict-mode / maintenance 豁免改路由元数据驱动（消除两份硬编码列表不一致）。

---

## 4. 范围分级：MUST / SHOULD / WON'T

### 4.1 MUST（v1 GA 前必须）

> 不做不能发 v1.0。详细任务见 `backlog.md` M0 / M1 / M2 三个里程碑。

**契约 / 序列化（6 项）**

- M0-C1 客户端×后端第四轮 cross-validator 审计（合入 v0.6.0-beta.1–beta.4 变更）
- M0-C2 `docs/openapi.yaml` 改 utoipa 自动生成 + CI 防漂移
- M0-C3 SSE 事件表补齐 4 个缺失事件（`new_llm_suggestion / release_available / update_progress / probe_request / probe_confirm`）
- M0-C4 `Deprecation` / `Sunset` header 中间件 + 集成测试
- M0-C5 `/api/v1/*` 立即 410 Gone（已拍板）+ release notes 公告 deprecated
- M0-C6 修 `scripts/verify-*-auto-update*.sh` 双通道契约

**发布流（4 项）**

- M0-R1 `release.yml` 加 `pre-release-tag-lint`（tag 含 `-` 必须匹配 `v\d+\.\d+\.\d+-(alpha|beta|rc)\.\d+`）
- M0-R2 minisign 签名 step + updater 端验签（已拍板）
- M0-R3 自更新失败自动回滚增强（fork-exec 后 60s 内子进程 `/health` 不 200 → 父进程回滚）
- M0-R4 apply 进入 swapping 自动开 maintenance、completed/failed 自动关

**文档（8 项）**

- M0-D1 项目根 `README.md`
- M0-D2 `CHANGELOG.md`（v0.1.2 → v0.6.0-beta.4 全量，脚本化）
- M0-D3 `SECURITY.md`
- M0-D4 `CONTRIBUTING.md`（Conventional Commits + PR 模板 + 本地三件套）
- M0-D5 更新 `docs/auto-update.md`（双通道 + `channel` 参数 + 异步 apply + applyTask）
- M0-D6 VitePress sidebar 收录运维 / 开发者参考文档
- M0-D7 `docs/runbook/`（5 篇：backup-restore / incident-response / key-rotation / scaling / monitoring-setup）
- M0-D8 `docs/user/`（4 篇：installation-ios / installation-web / faq / privacy）

**架构 / 代码债（7 项）**

- M1-A1 删除 `services/LearningService` + `WordbookService`；`AdminService` 整合（arch-scout P0-1）
- M1-A2 `AMASEngine` 锁中毒防护（`unwrap_or_else(PoisonError::into_inner)` 或 `parking_lot`）（arch-scout P0-6）
- M1-A3 删 4 个 stub worker（arch-scout P0-4）
- M1-A4 sled-migration feature + binary + 依赖清理（arch-scout P1-4）
- M1-A5 cron scheduler 健康监测：每 worker 上报 `last_run_at` / `last_duration_ms` / `last_error`（perf-warden §6.2 / arch-scout P0-3）
- M1-A6 strict-mode / maintenance 豁免改路由元数据（arch-scout P1-5）
- M1-A7 `frontend/lib/queryClient.ts` 决策落地：**删除 + 现状架构定型**（仓内现有 33 处 `createResource` 全部保留，不再用 @tanstack/solid-query）（arch-scout P0-5）

**性能 / 监控（5 项）**

- M0-P1 `/metrics` 端点（OpenMetrics 文本格式，admin 鉴权；axum-prometheus 集成）：QPS / 延迟 P50/95/99 / SSE 连接数 / DB 大小 / worker last_run_at（perf-warden §6.1）
- M0-P2 `.env.example` 与代码默认对齐（`SQLITE_POOL_SIZE=16` 等）+ 发版前自检（perf-warden §6.1 / signal-miner §5.7）
- M0-P3 `monitoring_aggregate` worker 实装或加 retention（perf-warden §5.2 P10）
- M0-P4 5xx 错误率告警接 admin SSE（基于 M0-P1 metric）
- M0-P5 自更新 phase 超时（downloading / verifying / extracting > 5 min 主动 abort + rollback）（perf-warden §6.1）

**合规 / 安全（3 项）**

- M1-G1 GDPR Article 20 数据导出端点 `GET /api/users/me/export`（machine-readable 全量数据；已拍板 v1 必做）
- M1-G2 AMAS LLM 顾问月度硬性成本上限 + admin 告警（已拍板 v1 必做）
- M1-G3 feedback_items schema 升级（migration 加 `priority / status / assignee / resolved_at`）+ admin /admin/feedback 上线分类与处理（signal-miner §3.3.3 / §5.8）

**质量门验证（4 项）**

- M2-Q1 k6 压测 5 核心路径（登录 / 学习会话 / 复习提交 / favorites 列表 / SSE 建连），每路径 10k 请求验证 §9.1 SLO
- M2-Q2 Lighthouse + Web Vitals 实测前端首屏 LCP / TTI（admin 控制台）
- M2-Q3 客户端契约第四轮 cross-validator **0 P0 / 0 P1**
- M2-Q4 连续 1 周（v1.0-rc 公开后）无 P0 回归

### 4.2 SHOULD（v1.0 内尽量，但不阻塞 GA）

- S1 `routes/learning.rs` 1398 行 + `routes/records.rs` 849 行按 lifecycle 拆 4–5 子文件（arch-scout P1-1）
- S2 records → AMAS 事件总线化（消除手动 rollback 的非原子性）（arch-scout P0-7，长期改造）
- S3 nginx sample.conf + TLS（certbot）runbook（release-keeper O2）
- S4 maintenance 模式 admin UI 开关（`/admin/settings` 现有面板加切换）（release-keeper O4）
- S5 升级历史审计表 `update_audit_log`（release-keeper O5）
- S6 ErrorBoundary 接 Sentry / openobserve（arch-scout P1-6）
- S7 health 端点 `error_rate` 字段实装（基于 M0-P1）（arch-scout P2-1）
- S8 3 条前端 `it.skip` 复活或删除（arch-scout P1-7）
- S9 `release-calendar.md`（跨三仓发版与兼容窗口）（signal-miner §5.9）

### 4.3 WON'T（v1.0 不做，明确推后）

- W1 协作 / 班级 / 多人（v2，signal-miner §4）
- W2 商业化 / 订阅 / 付费墙（v2）
- W3 切 Postgres / 拆库（v2，perf-warden §6.1 决议）
- W4 多实例 / leader 选举 / 集群升级（v1.1，release-keeper §6.5 H1）
- W5 灰度发布（按用户百分比 / 客户端版本切流）（v1.1）
- W6 per-user algorithm fine-tune（FSRS Optimize 按钮风格）（v2，signal-miner §4）
- W7 LSTM / Birdbrain 级自适应（v2）
- W8 OAuth 第三方接入 / Quizlet Partner Platform 风格（v2）
- W9 用户互助 / 助记内容共创（v2，法务成本高）
- W10 DB 备份外迁（S3 / rsync）（v1.1，release-keeper §6.4 O3）

---

## 5. 已拍板决策（用户已确认）

| # | 决策 | 来源 |
|---|---|---|
| D1 | 本仓 v1 = 后端 + admin + API SDK v1.0（学习端 / 移动端独立） | 2026-05-21 用户拍板 |
| D2 | `/api/v1/*` 立即 410 Gone + 12 个月窗口后删；release notes 公告 deprecated | 2026-05-21 用户拍板 |
| D3 | v1 GA 节奏：**质量门决定**，不预设固定时间窗 | 2026-05-21 用户拍板 |
| D4 | OpenAPI 自动生成（utoipa 注解链）v1 必做 | 2026-05-21 用户拍板 |
| D5 | GDPR Article 20 数据导出端点 v1 必做 | 2026-05-21 用户拍板 |
| D6 | minisign / cosign 二进制签名 v1 必做 | 2026-05-21 用户拍板 |
| D7 | AMAS LLM 顾问月度硬性成本上限 + 告警 v1 必做 | 2026-05-21 用户拍板 |
| D8 | v1 不切 Postgres、不拆库、不上 HA | perf-warden §6.1 决议 |
| D9 | 算法默认采用 AMAS `nearMiss[0]` 配置（pred +10.6% / mem +14% / ICI −25%） | docs/amas-tuning-2026-05-15/01-final-report.md |
| D10 | queryClient（@tanstack/solid-query）从 main.tsx 删除；现状 `createResource` 架构定型 | arch-scout P0-5 推荐 + 本 RFC 落实 |

---

## 6. 质量门（GA 触发条件，替代时间表）

> 用户已拍板"不定时机，以质量门决定"。以下四个门全部通过即可发 v1.0；任一未通过则停在对应 rc.X。

### 6.1 M0 门（→ v1.0-rc.1）：基础修复 + 安全网

**门禁条件**：
- 4.1 MUST 中的 M0-* 全部 completed（C1–C6 / R1–R4 / D1–D8 / P1–P5 共 23 项）
- `cargo test` 全过；`vitest` skip 数不增
- main 分支零编译警告（含 clippy）
- `docs/alignment.md` 第四轮审计：0 P0 / 0 P1

### 6.2 M1 门（→ v1.0-rc.2）：代码债 + 合规

**门禁条件**：
- 4.1 MUST 中的 M1-* 全部 completed（A1–A7 / G1–G3 共 10 项）
- M0 项无回归
- 删除/合并后的 services / workers / sled 链路通过 `cargo test --all` + e2e
- GDPR 导出端点的端到端测试覆盖（创建 → 学习 → 收藏 → 导出 → 删除账号 → 重新登录 0 数据）

### 6.3 M2 门（→ v1.0-rc.3）：质量门验证

**门禁条件**：
- 4.1 MUST 中的 M2-* 全部 completed（Q1–Q4 共 4 项）
- k6 5 路径压测达到 §9.1 SLA 目标
- Lighthouse admin 控制台 LCP < 2.5s（中端 Android）
- minisign 验签 e2e：发 rc.3 时仅本地私钥签名能通过 updater 验证

### 6.4 GA 门（→ v1.0）：稳态验证

**门禁条件**：
- v1.0-rc.3 公开发布后连续 **7 天无 P0 回归**
- `/metrics` 端点暴露的 5xx 错误率 < 0.1% / 滚动 1h
- 自更新成功率 > 95%（基于过去 30 天）
- 升级历史审计表（S5 / O5）有完整 rc.1 → rc.3 升级链记录

---

## 7. 风险登记册

> 合并自 perf-warden §5 + release-keeper §1.5/§2.2/§5 + signal-miner §5 + arch-scout §2/§3。
> 严重度：H = 已发生过事故 / M = 潜在但概率高 / L = 潜在但概率低。

| # | 风险 | 严重度 | 来源 | 缓解动作 | 关联任务 |
|---|---|---|---|---|---|
| R01 | 无路由级 HTTP latency / 5xx 监控（最大盲区） | H | perf-warden §5.2 P2 / §6.1 P0#1 | `/metrics` 端点 + 5xx 告警 | M0-P1, M0-P4 |
| R02 | `.env.example` POOL_SIZE=4 与代码默认 16 不一致 | H | perf-warden §5.2 P1 / §2.1 / signal-miner §5.7 | 同步 + 发版前自检 | M0-P2 |
| R03 | `monitoring_aggregate` WIP 永远 enabled=false，`engine_monitoring_events` 表只写不聚合 | H | arch-scout P0-4 / perf-warden §5.2 P10 | 实装或加 retention | M0-P3 |
| R04 | `download_client` 无 total timeout；GitHub CDN 死链可悬挂数十分钟 | H | perf-warden §5.2 P7 / release-keeper §2.2 P0 | phase > 5min abort + rollback | M0-P5, M0-R3 |
| R05 | `AMASEngine` 16 处 `lock().unwrap()`；锁中毒整库不可用 | M | arch-scout P0-6 | `unwrap_or_else(PoisonError::into_inner)` 或 `parking_lot` | M1-A2 |
| R06 | `records.rs` + AMAS 跨 store 非原子，手动 rollback 是手抖 | M | arch-scout P0-7 | 事件总线化（v1 内仅文档化承诺，实装推 S2） | S2 |
| R07 | systemd `Restart=on-failure` 对 fork-exec 后父进程 exit(0) 不重启 | H | feedback_admin_self_update_pitfalls / release-keeper §2.2 P0 | install.sh 明文 + Runbook | M0-D5, M0-D7 |
| R08 | paginated 字段名前端易写错（`data.data` vs `items`） | H | feedback_paginated_field_name_check | utoipa 自动生成 + tsd 类型断言 | M0-C2 |
| R09 | strict-mode 豁免清单硬编码 + maintenance 列表不一致 | M | arch-scout P1-5 | 路由元数据驱动 | M1-A6 |
| R10 | strict-mode 真实头 / 错误码与 release notes 表述不一致（`x-device-platform` vs `X-Client-Platform`） | M | wordforge_prod_deployment | 常量作 codegen 源 + CI 一致性检查 | M0-C2, M0-D5 |
| R11 | sled-migration binary 长期不构建，feature gated | L | arch-scout §3.2 | 删 binary + feature + 依赖 | M1-A4 |
| R12 | 4 个 stub worker 占模块位 + 配置位 | L | arch-scout P0-4 | 删除 | M1-A3 |
| R13 | `services/` 层 3 个空壳，命名误导新人 | L | arch-scout P0-1 | 做减法 | M1-A1 |
| R14 | `frontend/lib/queryClient.ts` 死依赖 30–50KB | L | arch-scout P0-5 | 已拍 D10：删除 | M1-A7 |
| R15 | cron scheduler 内部 panic 静默停摆，admin 看不出 | M | arch-scout P0-3 | `last_run_at` + scheduler health gauge | M1-A5 |
| R16 | release.yml prerelease 锚定 `ref_name` 含 `-`；tag 命名错误致 stable/beta 误标 | M | signal-miner §5.3 / release-keeper §1.3 | `pre-release-tag-lint` | M0-R1 |
| R17 | GH 账号被入侵 → sha256 校验失效，updater 拉伪 release | M | release-keeper §2.2 P2 / auto-update.md:140 | minisign 签名 + updater 验签 | M0-R2 |
| R18 | 自更新失败 fork-exec 后子进程起不来不会回滚 | H | release-keeper §2.2 P0 | 60s `/health` 自检 + 父进程回滚 | M0-R3 |
| R19 | apply swapping 期间不开 maintenance，5xx 漏给前端 | M | release-keeper §2.2 P1 | 进 swapping 自动开 / 结束自动关 | M0-R4 |
| R20 | OpenAPI v0.4.3 落后 4 release + 极简 stub + 不在 CI | H | release-keeper §1.5 / §4.4 | utoipa 自动生成 + CI diff | M0-C2 |
| R21 | SSE 事件表过期缺 4 个事件 | M | release-keeper §1.5 / §4.3 | 同步事件表 + 测试 | M0-C3 |
| R22 | `verify-*-auto-update*.sh` 单通道契约 v0.6.0-beta.3+ 立即破 | H | release-keeper §1.5 | 改双通道 + `channel` 字段 | M0-C6 |
| R23 | README / CHANGELOG / SECURITY / CONTRIBUTING 全缺，GitHub 首页空白 | M | release-keeper §4.3 | 4 篇顶级文件 | M0-D1–D4 |
| R24 | feedback_items schema 不足以支撑反馈中心（缺 status / priority / assignee） | M | signal-miner §5.8 / §3.3.3 | migration + admin UI | M1-G3 |
| R25 | AMAS LLM 顾问成本上限是软配置（运营忘改可炸账单） | M | signal-miner §5.10 | 硬性月度上限 + 告警 | M1-G2 |
| R26 | iOS / wordforge-web 跨仓发版无协同日历 | L | signal-miner §5.9 | release-calendar.md | S9 |
| R27 | 本地 dev DB 不跟进 schema head（feedback 表本地无数据） | L | signal-miner §5.2 | dev fixture migrate 后注入样本 | （S，可选）|
| R28 | systemd unit 部署侧手改不入仓 | M | signal-miner §5.6 / feedback_admin_self_update_pitfalls | `deploy/wordforge.service.tmpl` 入仓 + install.sh 引用 | M0-R3 副产 |
| R29 | rate_limit 按 IP 不区分用户，NAT 多人互拖 | L | perf-warden §5.2 P5 | 推 v1.1 |（S，可选）|
| R30 | SSE 1000 上限 / 文件描述符未实测 | L | perf-warden §5.2 P4 | M2-Q1 子项压测 | M2-Q1 |
| R31 | metrics 6 桶 fixed 精度过低，真实 P99 > 10ms 跑出桶 | L | perf-warden §5.2 P11 | 用 `/metrics` 端点的 prometheus histogram 替代 | M0-P1 副产 |
| R32 | telemetry 无 backpressure，客户端高频灌可打爆 monitoring 5% 链路 | L | perf-warden §5.2 P12 | per-user 限频 + payload size guard |（S，可选）|
| R33 | 3 条前端 `it.skip` 无 issue link，技术债隐形 | L | arch-scout P1-7 | 复活或删除 + issue 记录 | S8 |
| R34 | `routes/learning.rs` 1398 行 + `routes/records.rs` 849 行单文件膨胀 | L | arch-scout P1-1 | 拆 4–5 子文件 | S1 |
| R35 | `store/operations/extras.rs` 1134 行 catch-all 滚雪球 | L | arch-scout P1-3 | 按主题拆分 |（v1.1）|
| R36 | 14 条 migration 跳号且无回滚 | L | arch-scout P1-2 | 推 v1.1（涉及全表回滚设计） |（v1.1）|

---

## 8. 决策点（开放问题，需后续拍板）

> 用户已拍板的列入 §5。以下是 RFC 中尚未拍板但需要在 backlog 推进时回头决策的开放问题。

| # | 决策点 | 选项 | 倾向 |
|---|---|---|---|
| O1 | M0-D7 Runbook 5 篇是否一次性 3 人日完成，还是按需补 | (a) 5 篇齐发 (b) 按事故倒推 | (a) 一次性，避免事故时才发现缺 |
| O2 | M0-C2 utoipa 注解链是否扩展到所有 60+ 端点，还是先覆盖 v1 stable 档 | (a) 全量 (b) 先 stable 档 25 个 | (b) 先 stable 档，beta/internal 后补 |
| O3 | M1-G1 GDPR 导出格式 | (a) JSON Lines (b) ZIP 多文件 (c) SQLite 单库 | (a) JSON Lines，便于机器读 |
| O4 | M1-G2 LLM 月度硬上限默认值 | (a) ¥50 (b) ¥200 (c) 由 admin 设置 | (c) admin 设置 + 默认 ¥100 |
| O5 | M0-R2 minisign 公钥嵌入方式 | (a) 编译期常量 (b) 运行时从 release.yml 同源 fetch | (a) 编译期常量（最强保证） |
| O6 | M1-A7 queryClient 删除后，未来若需要请求缓存如何处理 | (a) 自实现 cache map (b) 加回 query 但仅用于 admin 大表 | (a)，与 D10 一致 |
| O7 | M2-Q1 k6 脚本是否入仓 `.github/workflows/` 周期跑 | (a) 入仓周跑 (b) 入仓仅手动 dispatch (c) 不入仓本地跑 | (a) 周跑，自动捕回归 |
| O8 | API 弃用窗口长度 | (a) 6 个月（2 个 minor） (b) 12 个月 (c) 24 个月 | (a) 6 个月，已写入 §9.3 |
| O9 | v1.0-rc.X 公开通道 | (a) 仅 beta 通道发 (b) stable + beta 双发但 stable 不标 latest | (a) 仅 beta，避免误升级用户群 |
| O10 | M2-Q4 "无 P0 回归"判定 | (a) 仅看 issue tracker (b) issue + admin SSE 告警 + metrics 阈值 | (b) 三源合一 |

---

## 9. SLO / SLA 提案

> 详细 P50/P95/P99 表见 `03-perf-warden.md` §3 / §7；本 RFC 提取 GA 公告承诺级别。
> 适用环境：单实例阿里云 ECS（≥ 2 vCPU / 4 GiB RAM），用户规模 1k–5k DAU。
> 置信度全部 L–M（无生产实测）；M2-Q1 k6 压测前不公开承诺，rc.3 验证后写入 release notes。

### 9.1 用户路径 SLA（M2-Q1 验证后生效）

| 指标 | 目标 |
|---|---|
| `/api/auth/login` P95 | < 300 ms |
| `/api/learning/sessions` POST P95 | < 250 ms |
| `/api/learning/sessions/:id/complete` P95 | < 300 ms |
| `/api/records`（单条）P95 | < 150 ms |
| `/api/words/batch-get` P95 | < 120 ms |
| `/api/favorites?page` P95 | < 100 ms |
| `/api/realtime/events` 建连 P95 | < 500 ms |
| admin 控制台首屏 LCP（中端 Android） | < 2.5 s |
| admin 控制台路由切换 TTI | < 1 s |

### 9.2 全站 SLA

| 指标 | 目标 |
|---|---|
| 月度可用性 | 99.5%（≤ 3.6 h down/月） |
| 5xx 错误率（不含 429） | < 0.1% / 滚动 1h |
| 4xx 业务错误率 | < 2% |
| 自更新成功率 | > 95% / 季度 |
| 自更新 apply（不含下载）P95 | < 90 s |
| SQLite 库大小（GA 触发预警） | < 5 GiB |
| 单实例稳态 QPS | ≥ 100 req/s |
| 单实例峰值 QPS | ≥ 300 req/s |
| SSE 并发上限 | ≥ 1000 active |

### 9.3 算法 SLA（已实测，可承诺）

| 指标 | 目标 | 当前实测 |
|---|---|---|
| AMAS prediction_composite | ≥ 1.10 vs DEFAULT | 1.1062（✅） |
| AMAS DHP expectedMemory | ≥ 3000 | 3154.6（✅） |
| ICI（校准） | < 0.05 | 0.0379（✅） |
| `/api/amas/process-event` P95 | < 50 ms | 待 M2-Q1 实测 |

**已知折让**（写入 v1.0 release notes "已知限制"）：
- DHP `targetCount` 比 baseline 低 13.4%（GA 不阻塞，监控演化）
- 算法 split=test 泛化验证待补一次

---

## 10. 对外契约稳定性承诺（v1 stable / v1beta / v0-internal）

> 完整 60+ 端点 / 9 SSE 事件分档表见 `04-release-keeper.md` §3.2。本节只列承诺与变更纪律。

### 10.1 三档定义

- **v1 stable**：v1 全生命周期禁止破坏性变更；新增字段必须可选 + default；删除字段必须经 deprecation 流程。
- **v1beta**：可加新必填字段、可改 enum 值；变更必须出现在 release notes "Breaking" 段。
- **v0 / internal**：无承诺；admin 自用 / 运维诊断工具；schema 可随意改。

### 10.2 Deprecation policy（已纳入 M0-C4）

1. v1 stable 端点弃用 = 公告 ≥ 2 个 minor 版本（约 6 个月，对应 O8(a) 默认值）。
2. 运行时信号：弃用端点 response header 加 `Deprecation: <date>` + `Sunset: <date>`（RFC 8594）。
3. 代码标记：handler 加 `#[deprecated(since = "v1.M.0", note = "use /new-path")]`；OpenAPI codegen 自动把 deprecated 端点标灰。
4. 删除时机：仅在 next major（v2）切换时移除；v1 全生命周期保留旧路径（即使返回 410 Gone 占位）。
5. 破坏性序列化变更：禁止在 v1 内引入；只在 major bump 时同步打掉客户端最低版本门（strict-mode `min_client_version`）。

### 10.3 立即生效条款

- `/api/v1/*` → 自 v1.0 发布起返回 410 Gone（已拍板 D2）；现有 4 个端点字段集永久冻结至删除日。
- `/api/admin/monitoring/check-update` → 自 v1.0 发布起 deprecated（被 `/admin/updates/*` 取代）；v1.1 删除。

---

## 11. 监控 / 报告链路

GA 后第一周每日（自动）：
- `/metrics` 抓取脚本输出每日 P50/95/99 + 错误率到 `~/.wordforge-bench/v1-ga/` 归档
- admin SSE 告警事件去重统计入日志

GA 后第一月每周（人工）：
- 5 路径 k6 抽样验证
- alignment.md 增量审计（若有客户端变更）

GA 后季度（人工）：
- 自更新成功率统计
- DB 大小巡检
- LLM 顾问月度成本上限利用率

---

## 12. 一句话总结

**WordForge 后端从 v0.6.0-beta.3 走向 v1.0 的距离 = `/metrics` 端点 + 4 个 stub 清理 + OpenAPI 自动生成 + 4 顶级文件 + 5 篇 runbook + minisign + GDPR 导出 + LLM 硬上限 + 第四轮契约对齐 + k6 五路径压测 + 7 天稳态观测**。质量门一过即发；无固定时间表，但每一步都有可执行任务（见 `backlog.md`）。
