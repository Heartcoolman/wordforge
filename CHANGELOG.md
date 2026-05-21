# Changelog

所有版本变更记录均在此文件。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

后续每次发版在文件顶部 append 最新条目；全量重建可运行 `bash scripts/build-changelog.sh`。

---

## [Unreleased] — M1-A3

### 移除

- **删除 4 个 stub worker**（`monitoring_aggregate` / `etymology_generation` / `embedding_generation` / `word_clustering`）
  - 这 4 个 worker 自引入起始终 `enabled: false`，从未在生产环境运行
  - `monitoring_aggregate`：依赖未实现的 `insert_monitoring_timeseries` 存储接口；已由 M0-P4 `error_rate_watchdog` 覆盖实时监控需求
  - `etymology_generation` / `embedding_generation` / `word_clustering`：依赖外部 LLM provider，provider 集成未完成；功能规划移至 M2+ 阶段重新设计
  - 删除对应源文件（4 个 `.rs`）及 `mod.rs` 中的模块声明、`WorkerName` 枚举变体、`planned_jobs()` 条目、`register_jobs()` match arm

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
