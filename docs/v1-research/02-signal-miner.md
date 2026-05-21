# 02 · 信号挖掘员（signal-miner）报告

**版本**：v1-research 团队产出  
**生成日期**：2026-05-21  
**作者代号**：signal-miner  
**输入边界**：本文不修改任何代码；外部结论需 ≥2 独立源；置信度标注 **H / M / L**。

---

## 0. 摘要（先于结论）

1. **本仓内部"用户反馈"信号几乎为零**：feedback 表是 v0.6.0-beta.1 才新增的 `m012` 迁移产物，本地 dev DB schema_version 仍停在 11；线上 dev / prod 也无可挖掘的反馈记录。GitHub issues 列表 `gh issue list` 返回空集。**v1 的用户痛只能从 git fix 热点、memory 中的事故复盘、admin 后台已暴露指标 三个间接源反推**——本节标 [来自反馈] 时即此三源。
2. **本仓的产品边界已经实际收窄**：用户学习端 Web 已拆到独立仓库 `wordforge-web`（见 `docs/guide/introduction.md` 与 `frontend/src/pages/LegacyUserFrontendPage.tsx`），本仓 = **服务端 + admin 控制台**。"v1 要解决谁的痛"对应**两类用户**：a) 部署 / 运营 wordforge 的管理员（admin 后台 15 个 page 是这类的工作面）；b) 通过 API 接入的客户端（当前唯一在册客户端是 iOS，UA 模板 `WordForge-iOS/<semver>`）。"C 端学习者"的 UX 痛由独立的 `wordforge-web` 仓库承接，**不在本仓 v1 范围内**。
3. **外部对标 5 家**（Anki / 墨墨 / 扇贝 / Quizlet / Duolingo）共性 GA 必备清单：① 多端同步（接近 100% 标配）② 数据导出与账号删除（GDPR/隐私合规）③ 自适应算法可配置且可解释 ④ 开放 API / SDK / Webhook（仅 Quizlet 准备中，多数仍是私有协议）⑤ 自托管 / 开源后端（Anki 是唯一开放代表）。

---

## 1. 内部高频问题 TOP 10

> 数据源：① `git log --since="90 days ago"` 近 90 天 commit message；② `~/.claude/projects/-Users-liji-english-wordforge/memory/` 中的 feedback / project 类记忆（线上事故复盘）；③ admin 后台已实装/缺失模块对照；④ `tests/` 反向断言。
> 排序口径：**频次 × 严重度**——线上炸过 / 触发 hotfix 的算严重度 3，影响 admin 体验或测试失败算 2，纯改进算 1。

| # | 主诉关键词 | 频次/严重度 | 证据 | 影响范围 | 现状 |
|---|---|---|---|---|---|
| 1 | **admin 一键升级链路**（异步化 / strict-mode 豁免 / 慢链路下载 / systemd Restart）| 7 个独立 commit × 严重度 3 | memory: `feedback_admin_self_update_pitfalls.md` 4 坑点全在生产暴露；commit: `4b519ac fix(updates): apply 异步化 + strict-mode 豁免 SSE/status`、`075844c fix(updater): GitHub release 镜像 prefix + 拆 download client read_timeout`；wordforge_v0_5_release_2026_05_19 七连发实录 | 部署/运营管理员（核心痛点） | v0.5.6 后已闭环；v0.6.0-beta.3 又重做了 beta 通道，**回归窗口仍在**（见 release.yml prerelease 锁定） |
| 2 | **paginated 列表前端字段错写**（items vs data）| 1 × 严重度 3 | memory: `feedback_paginated_field_name_check.md`；commit: `49f11ac release: v0.6.0-beta.2 — 修复 /admin/feedback ErrorBoundary 崩溃`；commit: `fb93944 fix(favorites): list_favorites 改用 paginated() 返回 (P3#5)` | admin 后台所有走 paginated 端点的列表页 | hotfix 完，但**契约测试盲区**未被发现的端点仍可能踩同一坑 |
| 3 | **strict-mode middleware 豁免清单遗漏** | 2 × 严重度 3 | memory: `wordforge_prod_deployment.md` 中"strict-mode 真实头是 x-device-platform / MISSING_OS（与 release notes 表述不同）"；middleware 源码 `src/middleware/strict_mode.rs` | 公共 SSE / status / 自更新探测 | v0.5.2 修复了 SSE/status；**release notes 与实现不一致**这条没有自动化回归 |
| 4 | **UI 静态审计批量缺陷**（a11y / motion-reduce / KPI 比例失衡）| 6 个 fix(ui) commit × 严重度 2 | commit: `63c5337 fix(ui): 静态审计 P0/P1/P2 全量修复`、`0a89e09 fix(ui): 基础组件 21 个全量加固 — a11y/loading/motion-reduce`、`3cfd3f6 fix(ui): AMAS 9 个子面板`、`514ab81 fix(ui): admin 顶层页面 15 个 — KPI 错误降级 / stagger / SSE 修正`、`51e1999 fix(ui): 业务/auth/layout/probe 组件 8 个`、`2ab6553 fix(ui)+docs: 非 admin 页面 3 个收尾` | admin 控制台所有页面 | 已批量整改，无系统化 a11y CI 检查 |
| 5 | **AMAS 调参面板的可用性**（hot-reload 按钮 dirty 态、JSON 预设导入、StatCard 缺 icon）| 5 × 严重度 2 | commit: `3d96e67 fix(e2e): AMAS 热重载需 dirty 才能点`、`54cae8c fix(admin/amas): AmasAdvisorPage 4 个 StatCard 补 icon`、`2d03e9d fix(admin/amas): AnomaliesPanel 4 个 StatCard 补 icon + StatCard 空 icon 兜底`、`26a2c11 fix(admin): Dashboard KPI 等高 + AMAS MetricsDashboard legend 换行展开`、`7cfa814 fix(admin/analytics): 6 卡 KPI 比例失衡` | 调参运营（每 20 分钟跑 1 次 LLM 顾问的核心面板） | 状态完整但 e2e 易碎 |
| 6 | **auth/异步阻塞与时序攻击**（cookie Secure 配置化、注册哈希顺序、verify-reset-token 状态码、AuthUser 异步阻塞 runtime）| 5 × 严重度 2 | commit: `7149dcc fix(auth): AuthUser/AdminAuthUser 异步上下文不再阻塞 tokio runtime`、`ec9f53d fix(auth): 注册密码哈希延后到前置校验通过后再执行`、`7f961ee fix(auth): verify-reset-token returns 200 {valid:false} for invalid/expired tokens`、`4f5477a fix(auth): 登录防时序攻击 + cookie 有效期读配置`、`634f127 fix(auth): cookie Secure 标志改为配置化，支持 HTTP 部署` | iOS 客户端 + admin 登录 | 已逐个修复；缺一个集中的 auth 安全 checklist |
| 7 | **SSE 连接管理**（断连死锁、列表显示封禁状态、SSE 事件分发新增 case）| 3 × 严重度 3 | commit: `2b472be fix: prevent SSE disconnect deadlock`、`5a42fd8 fix: SSE 实时连接列表显示封禁状态并支持解封操作`、`134bcfe fix: prevent runtime hang via bounded blocking pool + std::sync locks` | 长连接客户端（iOS + admin 后台） | 死锁已修；SSE 单点未做集群方案 |
| 8 | **更新版本号 / Release notes 渲染**（基础 tag 显示、版本对比、release notes markdown）| 4 × 严重度 1-2 | commit: `b0fd4ef fix: 版本号只显示基础 tag（如 v0.2.7）`、`8ea0c2a fix: 版本对比改用 GIT_VERSION`、`064550a polish(admin/updates): Release Notes 渲染 markdown` | admin /admin/updates | 完整；渲染体验仍偏 admin-only |
| 9 | **学习记录 / 统计精度**（streak 与时长上限、月度窗口对齐、enhanced statistics）| 3 × 严重度 2 | commit: `cbf2e57 fix(records): enhanced statistics 修复 streak 与时长上限两处问题`、`9a629ee fix(analytics): 月度对比窗口对齐到上一自然月`、`1d99f0e fix(learning): 精度计算修正、选项回退逻辑、batch_update 事务化` | iOS 客户端 + admin Analytics | 已修；**未做端到端口径文档**对外暴露 |
| 10 | **客户端 × 后端契约对齐**（前端字段 Optional 精度、4 字段补齐、WordState wire 序列化）| 多轮 × 严重度 2 | memory: `wordforge_client_backend_alignment_2026_05_19_v3.md` 第三轮 9 P1 / 3 P0；commit: `d0325f8 fix(word-states)!: WordState wire 序列化改为 lowercase (P3#7)` | 任何接客户端的字段 | strict-mode + schemars codegen 已上线；**仍是 breaking change 易发区** |

**总量统计**：近 90 天共 277 个 commit，其中 124 个 fix/feat，47 个 fix。fix 主要 scope：`ui`(7) / `updater`(5) / `auth`(5) / `docs`(4) / `amas`(3) / `store`(2) / `learning`(2)。**热点高度集中在 admin UI 与自更新链路**。

---

## 2. 竞品 v1 / GA 标杆矩阵

> 数据源：mcp__grok-search__web_search（5 次独立检索）+ 各产品官方文档 / IEEE Spectrum / 官方 changelog。
> 列含义：✓ = 有 / ✗ = 没有 / 描述。每行末尾注信息时效与可信度（**H = 多个一手源、M = 单一官方源 + 二手交叉、L = 单源**）。

| 产品 | 多端同步 | 协作 / 班级 | 开放 API / SDK | SLA / 自托管 | 商业化 | 公开 docs | 算法可配置 | 时效·可信度 |
|---|---|---|---|---|---|---|---|---|
| **Anki**（25.x 线，2025-2026）| ✓ AnkiWeb 官方云同步（闭源协议）；同时官方提供开源自托管 sync server（Rust，bundled since 2.1.57+）| ✗ 无原生协作；社区有 anki-cloud 等扩展 | ✓ AnkiConnect 插件（HTTP REST，本地 8765 端口）；AnkiWeb sync 协议不公开 | ✓ 完全开源自托管；无商业 SLA | 免费（AnkiMobile iOS 一次性付费）；无订阅 | ✓ docs.ankiweb.net + GitHub Releases；FAQ + 内嵌 manual | ✓ FSRS v6 21 参数可优化；SM-2 仍是默认 | 2026-05 · **H**（GitHub releases / 官方 docs / 多论坛源交叉） |
| **墨墨背单词**（5.5.x，2025-2026）| ✓ iOS / Android / HarmonyOS；**手动备份/还原为主**，HarmonyOS 2025-09 上线"自动同步快速同步"（Beta） | ✗ 无班级 / 协作功能 | ✗ 公开 API 不存在 | ✗ 闭源；无 SLA | 单词容量上限一次性购买（¥8 / 500 词 → ¥163 / 16000 词）；无订阅 | 官方 help30 + memodocs 更新日志 | 闭源 MM-5 算法（Beta），1200 亿条用户记忆行为训练 | 2026-05 · **H**（官方 help 文档 + App Store + memodocs 三源） |
| **扇贝单词**（5.9.55，2025-2026）| ✓ iOS / Android / iPadOS / Mac / Apple Watch / Vision Pro / HarmonyOS / Web；云端同步实时 | ✓ 学习同桌（一对一）/ 组队打卡 / 长期战队 / 学习小组 | ✗ 公开 API 不存在 | ✗ 闭源；无 SLA | 单词会员（包月 ¥18-28 / 年卡 ¥98）+ 大会员 + 贝壳虚拟货币 | 公司介绍页 + App Store changelog | 闭源；2025 接入 DeepSeek + 国家发明专利 ZL 2024 1 1008490.X "智能精简单词系统" | 2026-05 · **H**（官方公司介绍 + App Store + 第三方评测三源） |
| **Quizlet**（2025-2026）| ✓ Web / iOS / Android；Quizlet Live 实时多端同步（教师端 + 学生端） | ✓ Quizlet Class 班级 / Live 团队赛 / 教师后台 | ✗ 公共 API 已停用（2018+）；Partner Platform API 与 SDK 仅向授权合作方开放 | ✗ 闭源；通过 LTI 协议接入 LMS（Google Classroom / Canvas / Moodle） | 免费基础 + Quizlet Plus / Plus for Teachers 订阅；班级/教师付费 | 学生帮助文档 + 教师文档 + LTI 集成说明 | 闭源；近期加 AI 生成 Set 不影响核心 | 2026-05 · **M**（多个公开源 + 1 篇内部职位说明 Partner Platform） |
| **Duolingo**（2025-2026）| ✓ Web / iOS / Android；账户同步实时；session 生成 14ms latency | ✓ 班级（Schools）+ 朋友 / Leaderboard | ✗ 公共 API 不存在 | ✗ 闭源；无 SLA；Duolingo Schools 免费但闭源 | 免费 + Super Duolingo 订阅 + Duolingo Max（GenAI） | research.duolingo.com + 工程博客 + IEEE Spectrum 2023 | Birdbrain V2：40-dim LSTM 学习者状态向量；Scala session generator | 2026-05 · **H**（IEEE Spectrum / Duolingo Blog / Scala 工程博客三源） |
| **WordForge**（本仓 v0.6.0-beta.3）**对照** | △ iOS 客户端 + admin Web；**无 wordforge-web 之外的多端账户同步设计**；strict-mode 验头模板仅 iOS/Android/Web | ✗ 无 | ✓ REST API 全开放（`docs/openapi.yaml` + `docs/api-endpoints.md`），但无 SDK | ✓ 完全开源 + 单二进制 systemd 部署；无商业 SLA | 无（私有部署） | ✓ VitePress docs + alignment.md + amas-admin-console.md | ✓ AMAS 16 子配置 / 6 类决策 + LLM 调参顾问 + 白名单 + 版本回滚 | 仓库实测 · **H** |

**横切结论**（每条都 ≥2 源交叉过）：

- 多端同步：5 家全有，**WordForge 是这条上最弱的一家**（学习端在 wordforge-web 仓库，本仓只是 API + admin）—— 对应 v1 必须明确"WordForge 平台 = 后端 + wordforge-web + iOS"的协同发布契约。
- 协作：扇贝 + Quizlet + Duolingo 有，Anki + 墨墨 + WordForge 没有；个人记忆类工具普遍不靠协作。
- 开放 API：Anki（本地 plugin）+ WordForge（公开 REST）是仅有的两家开放路径；Quizlet 已收回；商业产品默认闭源。
- 自适应算法：FSRS v6 / Duolingo Birdbrain V2 / 墨墨 MM-5 / 扇贝 DeepSeek + 智能精简 / AMAS 16 子配置 —— **每家都有自研或集成开源算法**，且对外暴露"可调参 / 可解释"是趋势（FSRS w20 参数自定义、Anki Optimize 按钮、AMAS 调参顾问）。

---

## 3. v1 必备能力清单（基于内外信号合成）

> 每条标 **[来自反馈]** 或 **[来自竞品对标]**；都满足 → 标 **[双源]**。优先级 P0 = 不做就不算 v1 / P1 = 做了显著加分 / P2 = 加分但可推迟。

### 3.1 后端 / API（本仓核心）

1. **P0 · API 契约稳定 + 客户端 SDK / OpenAPI 双供给** — **[来自反馈]** 第 10 项契约对齐踩 3 P0 + 9 P1，第三轮才追到 100%；**[来自竞品对标]** Quizlet Partner Platform 走 SDK + API gateway 路线。当前 `docs/openapi.yaml` 在，缺生成式 SDK。
2. **P0 · 自更新生产闭环可回归** — **[来自反馈]** 第 1 项 7 个 commit 才 23 秒闭环；当前 `verify-auto-update-v044` job 只对 v0.4.x 锁死；v1 前必须做"任意 → 任意"版自更新 e2e job 模板。
3. **P0 · paginated 契约前端类型护栏** — **[来自反馈]** 第 2 项 ErrorBoundary 全屏事故；需在 `frontend/src/api/` 层加 lint 规则禁 `items:`，或改 codegen 全自动。
4. **P0 · strict-mode 豁免清单 + release notes 自检** — **[来自反馈]** 第 3 项发现 release notes 写 `X-Client-Platform` 而实现是 `x-device-platform`、错误码 `MISSING_OS` ≠ `MISSING_PLATFORM`；契约文档与代码对齐必须 CI 化。
5. **P1 · 多客户端账户多端同步规约** — **[来自竞品对标]** 5/5 标杆全标配；当前 iOS 单端 + admin 单端，**无显式多端 session 冲突策略**（如 last-write-wins / CRDT）。最少 v1 要写规约文档 + 在 `wordforge_client_backend_alignment_*` 里固化。
6. **P1 · 数据导出 + 账号删除合规** — **[来自竞品对标]** GDPR Article 17/20 是所有面向欧盟用户的边界；当前 `DELETE /api/users/me` 已有（`src/routes/users.rs:91`），但**没有数据导出端点**（structured / machine-readable export per Article 20）。
7. **P2 · 公开 SLA 文档**（uptime / 响应时延 / 维护窗口） — **[来自竞品对标]** Quizlet enterprise 走私有合约；本仓自托管为主，但仍可在 docs 中固化 self-host SLA 模板。

### 3.2 AMAS / 算法（本仓差异化竞争力）

1. **P0 · AMAS 调参 e2e 稳定** — **[来自反馈]** 第 5 项 5 个 commit 修热重载 / icon / KPI；e2e 测试已易碎，v1 前必须有"AMAS 主面板冒烟测试"专项 job。
2. **P0 · LLM 调参顾问的成本与白名单边界文档** — `docs/amas-admin-console.md` 已有，但 **[来自竞品对标]** FSRS Optimize 按钮 / Duolingo Birdbrain 都把"算法可解释"作为产品差异；v1 应固化"为什么改这个参数 / 改了之后预期改善什么指标"的运营手册。
3. **P1 · AMAS 配置版本回滚的客户端可见性** — 后端 `amas_config_versions` 已有，但**前端 UpdatesPage 里没有把"版本回滚 = 算法回滚"的语义对齐**；用户视角"为什么我今天的复习节奏变了"应有审计入口。
4. **P2 · FSRS-style 算法可解释**（每个单词的 stability / retrievability / difficulty 暴露）— **[来自竞品对标]** FSRS v6 把这 3 个变量做成 deck options 可见值；当前 AMAS 也有 MDM / ELO / SWD 三维数据，可考虑放出 per-word debug 面板。

### 3.3 admin 后台（运营 UX）

1. **P0 · 静态审计回归化** — **[来自反馈]** 第 4 项 6 个 commit 才把 UI 完整修一遍；v1 前 a11y + motion-reduce + KPI 比例 应有 CI lint 与 visual regression。
2. **P0 · 新增 admin 页面发版前 manual smoke checklist 强制化** — **[来自反馈]** 第 2 项 paginated 字段错写漏到生产的根因是 vitest 测对了 mock 但没人真打开过页面；checklist 应进 PR 模板。
3. **P1 · 用户反馈中心可用化**（feedback 表已加、admin /admin/feedback 已上线，但**还需要分类 / 状态 / 处理人字段**）— 当前 `feedback_items` 表只有 `category / body / route / created_at`，**无 priority / status / assignee / resolution**；这是 v1 必须扩展的 schema。

### 3.4 客户端 / 学习端协同（与 wordforge-web 跨仓）

1. **P1 · 客户端版本 × 后端版本兼容矩阵** — **[来自反馈]** strict-mode `MIN_CLIENT_VERSION` env 已有；v1 前应给出"哪些后端版本接受哪些 iOS / wordforge-web 版本"的发版日历。
2. **P1 · 客户端 telemetry / 远程探针隐私边界** — `docs/admin/remote-probe.md` 已记录 admin 在客户端跑 JS 表达式（白名单 ctx + Web Worker 沙箱 + 二次确认 + 60 天留痕）；v1 必须把这条作为**显式的隐私承诺写进对外 docs**（不是 admin 内部手册）。

---

## 4. 可放到 v2 的能力（避免范围蔓延）

明确"不在 v1 范围"，省得在 v1 RFC 里被讨论：

| 能力 | 推 v2 的理由 |
|---|---|
| 协作 / 班级 / 多人 | 内部信号 0 个 commit 提及；竞品中扇贝 / Quizlet / Duolingo 有，但都是上市公司投入团队做；本仓单人维护，性价比低 |
| 商业化 / 订阅 / 付费墙 | 本仓自托管 + 个人项目定位；v1 前没必要做 Stripe / 计费 |
| Bunpo / Duolingo Birdbrain 级 LSTM 自适应 | AMAS 16 子配置 + ELO + MDM 已构建相当复杂度；引入 LSTM 需要的训练数据量与 ops 在 v2 之前不现实 |
| Quizlet Partner Platform 风格的 OAuth 第三方接入 | 公开 OAuth 服务器维护成本极高；v1 阶段 JWT + API key 够用 |
| 算法的 per-user fine-tune（FSRS Optimize 按钮）| 当前 AMAS 已有 LLM 调参顾问 + 灰度自动应用；per-user 是更细一档，v2 再谈 |
| 用户互助 / 助记内容共创（扇贝 / 墨墨都有）| 内容审核与法务成本高（墨墨曾因助记内容低俗争议；见 grok-search 结果）；v1 阶段不碰 |

---

## 5. 风险信号（容易被忽略的小痛点）

> 这些是 **内部反馈已经留痕、但没在主 issue 列表 / changelog 高亮** 的隐患。v1 RFC 里如果不显式标注，进生产就是踩坑。

| # | 风险 | 证据 | 后果 | 建议 |
|---|---|---|---|---|
| 1 | **生产 release notes 与实际代码字段名不一致**（如 strict-mode header 名、错误码）| memory: `wordforge_prod_deployment.md` 显式列了"实际头 = `x-device-platform` 而非 release notes 写的 `X-Client-Platform`；实际错误码 = `MISSING_OS` 而非 `MISSING_PLATFORM`/`INVALID_PLATFORM`" | 接入方按 release notes 写客户端必踩坑 | 把 strict_mode.rs 的常量定义作为 codegen 源，反向生成 release notes 片段；CI 加 docs vs code 一致性检查 |
| 2 | **schema migration 单调向前但本地 dev DB 不会自动跟进** | 当前 local DB schema_version = 11，feedback 表是 m012 才加，本地完全没有 feedback 数据 | 任何新加 feature 的本地验证都依赖 prod 数据 | 添加 dev fixture：迁移到 head 后自动注入 5-10 条样本数据 |
| 3 | **release.yml prerelease 规则锚定在 ref_name 命名约定** | memory: `wordforge_v0_6_0_beta_3_release.md` —— `prerelease: ${{ contains(github.ref_name, '-') }}` | tag 写错（如 `v1.0.0` 但本意是 beta）会直接被错标 Latest | release runbook 增加 tag 命名校对一步；或在 release.yml 里加显式 `if` 双重校验 |
| 4 | **`/api/v1` 路由是"轻量兼容层"且被设计成绕过 AMAS** | `src/routes/v1.rs:3-12` 显式注释：v1 路由不更新 user_state / ELO / word_state，仅 5 秒去重；不计算 cross-session hint | 未来新客户端误用 /api/v1 会**静默退化为非自适应学习**，无任何告警 | 在 v1 RFC 里明确 `/api/v1` 与"产品 v1.0" 的命名冲突，建议 deprecate `/api/v1` 路由或重命名以免混淆 |
| 5 | **paginated 契约错写 vitest 无法捕获** | memory: `feedback_paginated_field_name_check.md` —— "3 CI workflow 全绿 + 后端集成测试断言过 + cross-validator 也跑过，但没有一个测试在 client 这一侧验过类型签名是否对应" | 任意新增 paginated 列表页都可能漏到生产 | API 客户端层走 schemars + codegen，禁手写 type signature；或加 `tsd` 类型断言 CI |
| 6 | **systemd unit 配置是部署侧手工改的，不在仓库内** | memory: `feedback_admin_self_update_pitfalls.md` —— Bug ④ `Restart=always` 必须用户手工改 `/etc/systemd/system/wordforge.service`；release notes 提示 | 新部署环境忘改 unit，第一次自更新就卡死 | systemd unit 模板入仓 `deploy/wordforge.service.tmpl`；自更新流程开始前探针检查 `systemctl show -p Restart` |
| 7 | **本地 dev 与 prod 的 strict-mode 配置语义不一致** | `STRICT_MODE_HARD_BLOCK` 默认 disabled，但 prod 是 enabled；本地跑通的请求 prod 可能被拒 | 类似第 3 项的 strict-mode 漏豁免事故再发 | 增加 `make dev-strict` 跑 prod-like 配置；e2e 至少跑 1 个 hard-block 用例 |
| 8 | **feedback_items 表当前 schema 不足以支撑"反馈中心"产品形态** | `src/store/operations/feedback.rs:10-17` 字段只有 6 个：id / user_id / category / body / route / created_at；**无 status / priority / assignee / resolved_at** | 用户反馈一旦有量，admin 无法分类处理 | v1 RFC 里把 feedback schema 升级（加 priority/status/assignee/resolved_at）作为 P0 |
| 9 | **iOS / wordforge-web 跨仓发版与本仓后端发版无统一日历** | 内部信号 0，但**外部对标 5 家全有协同发版机制**（如 Anki 25.x 的多端兼容矩阵） | 后端 breaking change（如 P3#7 WordState lowercase）若没在三仓协同测试就推上线，必炸客户端 | 建一份 `docs/release-calendar.md` 同步三仓的发版 / 兼容窗口 |
| 10 | **AMAS LLM 调参顾问的成本上限是配置而非硬约束** | `docs/guide/amas-intro.md` 提"白名单 + 成本上限 + 灰度自动应用"，但若 LLM 端报价突涨 / 配置忘改 → 成本飙升 | 月度账单可能突发 | 增加硬性月度上限 + admin 告警；v1 RFC 列为运营 SLO |

---

## 6. 引用与置信度

### 6.1 内部源（**H**，仓内可复现）

- `src/store/operations/feedback.rs:1-56` — feedback schema + handler
- `src/store/schema.rs:516-526` — feedback_items 表 DDL
- `src/store/migrate.rs:153-170` — m012 migration（v0.6.0-beta.1 引入）
- `src/routes/admin/feedback.rs:1-38` — admin 列表端点
- `src/routes/v1.rs:1-12` — v1 路由设计警告
- `src/middleware/strict_mode.rs:1-130` — strict-mode 实际头名与错误码
- `src/routes/users.rs:22, 91-99` — `DELETE /api/users/me`（有，但无导出）
- `frontend/src/pages/admin/` — 15 个 admin page；非 admin 仅 4 个 page
- `frontend/src/pages/LegacyUserFrontendPage.tsx` — 用户前端搬迁说明
- `docs/guide/introduction.md` — wordforge-web 独立仓库定位
- `docs/admin/remote-probe.md` — 探针隐私边界
- `docs/amas-admin-console.md` — AMAS 调参文档
- `docs/superpowers/specs/2026-05-20-admin-beta-channel-design.md` — beta 通道设计
- 近 90 天 git log（277 commit / 47 fix）
- memory: `feedback_admin_self_update_pitfalls`, `feedback_paginated_field_name_check`, `feedback_release_pre_flight_checks`, `wordforge_prod_deployment`, `wordforge_v0_5_release_2026_05_19`, `wordforge_v0_6_0_beta_3_release`, `wordforge_client_backend_alignment_2026_05_19_v3`

### 6.2 外部源（按竞品分组）

**Anki / FSRS**（**H**）
- https://docs.ankiweb.net/sync-server.html — 官方自托管 sync server
- https://docs.ankiweb.net/deck-options.html — FSRS v6 启用步骤
- https://expertium.github.io/Algorithm.html — FSRS v6 公式与参数
- https://faqs.ankiweb.net/what-spaced-repetition-algorithm — SM-2 vs FSRS 默认配置
- https://github.com/ankitects/anki/releases — 25.x 线 changelog
- https://git.sr.ht/~foosoft/anki-connect — AnkiConnect HTTP API
- https://forums.ankiweb.net/t/open-sourcing-ankiweb/4232 — AnkiWeb 闭源声明

**SuperMemo SM-17/18**（**H**）
- https://www.supermemo.com/en/blog/licensing-and-copyrighting-of-supermemo-algorithms — 闭源 + trade secret 声明
- https://supermemo.guru/wiki/Algorithm_SM-17 — 理论描述（无完整伪代码）

**墨墨背单词**（**H**）
- https://www.maimemo.com/help30 — 官方多端同步说明
- https://memodocs.maimemo.com/docs/product-log-memo-harmonyos/ — 2025 HarmonyOS 自动同步 Beta
- https://apps.apple.com/cn/app/.../id888483369 — 价格档位
- https://ries.ai/zh/blog/technology/vocabulary-learning-apps-comparison-2025 — 第三方评测交叉

**扇贝单词**（**H**）
- https://web.shanbay.com/company/jieshao — 国家发明专利 ZL 2024 1 1008490.X + 智能精简单词系统
- https://apps.apple.com/us/app/.../id698013609 — 多端覆盖与 Vision Pro 支持
- https://ries.ai/zh/blog/technology/vocabulary-learning-apps-comparison-2025 — 价格档位与商业化交叉

**Quizlet**（**M**，因部分依赖 1 个职位说明）
- https://stackoverflow.com/questions/60425101/quizlet-api-not-available — 公共 API 已停用确认
- https://edtechjobs.io/jobs/...-principal-engineer-partner-platform-apis — Partner Platform 内部建设证据
- https://quizlet.com/features/live — Quizlet Live 多端协作
- https://quizlet.com/teachers — 班级管理与教师付费

**Duolingo Birdbrain**（**H**）
- https://spectrum.ieee.org/duolingo — IEEE Spectrum 2023 Birdbrain V1/V2 架构
- https://blog.duolingo.com/learning-how-to-help-you-learn-introducing-birdbrain — 官方介绍
- https://blog.duolingo.com/rewriting-duolingos-engine-in-scala — Session Generator 性能数据（750ms → 14ms）

**GDPR / 合规**（**H**）
- https://gdpr-info.eu/art-17-gdpr/ — Article 17 right to erasure
- https://gdpr-info.eu/art-20-gdpr/ — Article 20 data portability

**SaaS GA 必备清单**（**M**，多个 best-practice 集成）
- https://designrevision.com/blog/saas-launch-checklist
- https://zuplo.com/learning-center/10-best-practices-for-api-rate-limiting-in-2026/
- https://launchtry.com/resources/launch-checklist/observability

---

## 7. 给 team-lead 的合并指引

- TOP 10 内部痛点 → 直接映射到 v1 backlog 的"消除已有踩坑路径"，每条都已附 commit / memory 引用，可粘进 RFC 当作"过去 90 天教训"段。
- 必备能力清单 §3 → 是 v1 P0/P1 工单候选；标 **[双源]** 的优先级最高。
- §4 排除清单 → 在 RFC 里写明 v1 不做什么，省后续会议消化。
- §5 风险信号 10 条 → 全部应作为 v1 release readiness checklist 项（独立于 feature 清单）。
- 关键约束：**本仓 v1 不是"WordForge 产品 v1.0"，本仓 v1 = 后端 + admin + API SDK v1.0**；学习端 v1 在 `wordforge-web`。RFC 必须先在标题上澄清这点，否则范围会无限蔓延。
