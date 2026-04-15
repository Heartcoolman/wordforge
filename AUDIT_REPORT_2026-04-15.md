# 项目审计报告

> 审计日期：2026-04-15
> 审计范围：当前工作区（包含未提交改动）的后端、前端、测试与文档契约
> 审计重点：功能正确性、硬编码风险

## 结论摘要

- 后端主链路整体可运行，`cargo test` 在本地跑到集成测试阶段时发现 1 个真实契约失败：`tests/coverage_routes_http.rs` 中的用户画像习惯接口断言失败。
- 前端生产构建可以通过，但产物当前只覆盖管理后台；仓库中的学习端页面、API 包装器、E2E 与单测资产已和实际交付物明显脱节。
- 前端单测基线当前不可用：`npm test` 在当前环境下大面积失败，首先被测试初始化中的 `localStorage.clear()` 打断，导致业务层断言基本失去回归价值。
- 硬编码问题主要集中在 `worker` 调度、更新源地址、前端主题存储 key 以及一组环境默认值；其中运维类硬编码的风险高于算法阈值常量。

## Findings

### [P1] `/api/user-profile/habit` 的实际返回契约与 OpenAPI/前端约定不一致

- 分类：功能问题
- 证据：
  - [src/routes/user_profile.rs](/Users/liji/english/wordforge/src/routes/user_profile.rs:112) 的请求结构体虽然声明了 `camelCase`，但字段本身是 `preferred_hours` / `sessions_per_day` / `median_session_length_mins`
  - [src/routes/user_profile.rs](/Users/liji/english/wordforge/src/routes/user_profile.rs:164) 在 POST 响应中直接返回 snake_case JSON
  - [src/routes/user_profile.rs](/Users/liji/english/wordforge/src/routes/user_profile.rs:120) 的 GET 也会把 store 中的 snake_case JSON 原样返回
  - [src/store/operations/extras.rs](/Users/liji/english/wordforge/src/store/operations/extras.rs:109) 的存取层同样固定使用 snake_case
  - [docs/api/openapi.yaml](/Users/liji/english/wordforge/docs/api/openapi.yaml:1492) 和 [docs/api/openapi.yaml](/Users/liji/english/wordforge/docs/api/openapi.yaml:2896) 却把响应定义成 `preferredHours` / `medianSessionLengthMins` / `sessionsPerDay`
  - [tests/coverage_routes_http.rs](/Users/liji/english/wordforge/tests/coverage_routes_http.rs:539) 当前已能稳定复现，断言 `sessionsPerDay == 3.0` 时实际拿到 `null`
- 触发场景：
  - 用户更新学习习惯后，Web/移动端按 OpenAPI 或 TS 类型读取 camelCase 字段
- 影响：
  - 更新成功后前端无法正确回显
  - `GET /api/user-profile/habit` 与 `POST /api/user-profile/habit` 的消费方式都不稳定
  - 当前已经打穿后端集成测试
- 建议：
  - 路由层统一只对外暴露 camelCase
  - store 层保留 snake_case 也可以，但必须在路由层做明确转换
  - OpenAPI、Markdown 文档与集成测试统一到同一个契约
- 是否已有测试覆盖：有，`cargo test` 中的 `coverage_routes_http` 已失败

### [P1] 当前交付的 Web 前端实际上只覆盖管理后台，学习端路由已从成品中消失

- 分类：功能问题
- 证据：
  - [frontend/src/App.tsx](/Users/liji/english/wordforge/frontend/src/App.tsx:97) 只挂载 `/admin/*` 路由，其他路径统一走 `NotFound`
  - [src/routes/mod.rs](/Users/liji/english/wordforge/src/routes/mod.rs:90) 只给 `/admin` 配了 SPA fallback，非 admin 路径不会回退到 `static/index.html`
  - [docs/api/client-guide.md](/Users/liji/english/wordforge/docs/api/client-guide.md:10) 仍然把注册、登录、学习流程描述为正常的 Web 客户端链路
  - [frontend/e2e/auth.spec.ts](/Users/liji/english/wordforge/frontend/e2e/auth.spec.ts:5) 和 [frontend/e2e/learning-flow.spec.ts](/Users/liji/english/wordforge/frontend/e2e/learning-flow.spec.ts:5) 仍然要求 `/login`、`/register`、`/learning` 可访问
- 触发场景：
  - 浏览器直接访问 `/login`、`/register`、`/learning`、`/wordbooks` 等学习端 URL
- 影响：
  - 当前 Web 构建产物与 README / E2E / 文档宣称的“全栈学习平台”不一致
  - 任何用户侧 Web 入口都会直接落到 404 或静态文件缺失
- 建议：
  - 如果项目目标已改为“后台前端 + API 服务”，需要同步下线这些文档与测试资产
  - 如果仍然要保留学习端 Web，必须同时恢复前端路由树和服务端 SPA fallback
- 是否已有测试覆盖：有，但已与当前成品脱节；E2E 期望与路由树不一致

### [P2] 前端用户侧 API 包装层已经残缺，认证与画像相关资产存在明显契约漂移

- 分类：功能问题
- 证据：
  - [frontend/src/api/auth.ts](/Users/liji/english/wordforge/frontend/src/api/auth.ts:1) 当前只剩 `refresh()`
  - [frontend/tests/api/auth.test.ts](/Users/liji/english/wordforge/frontend/tests/api/auth.test.ts:27) 仍然要求 `login/register/logout/forgotPassword/resetPassword`
  - [frontend/src/types/userProfile.ts](/Users/liji/english/wordforge/frontend/src/types/userProfile.ts:15) 把 `learning-style` 定义成 `{ style, scores }`
  - [src/routes/user_profile.rs](/Users/liji/english/wordforge/src/routes/user_profile.rs:78) 实际返回的是 `{ processingSpeed, memoryCapacity, stability }`
  - [docs/api-reference/02-users.md](/Users/liji/english/wordforge/docs/api-reference/02-users.md:156) 与后端实现一致，说明前端类型已经漂移
- 触发场景：
  - 新接手的人按前端类型或测试补用户页/客户端
  - 以 TS 类型为准生成 SDK 或做页面联调
- 影响：
  - 前端资产表面上“有类型、有测试”，但并不能代表真实接口
  - 后续恢复学习端时，很容易在错误契约上继续开发
- 建议：
  - 先决定用户侧 Web 是否继续维护
  - 若继续维护，补齐真实 API 包装器并删掉失效类型
  - 若不再维护，删除或隔离对应测试、类型、E2E，避免误导
- 是否已有测试覆盖：有，但当前覆盖的是旧契约，不是现状

### [P2] 前端单测基线当前不可用，`npm test` 几乎在测试初始化阶段就被打断

- 分类：功能问题
- 证据：
  - [frontend/tests/setup.ts](/Users/liji/english/wordforge/frontend/tests/setup.ts:20) 在每个用例后无保护地调用 `localStorage.clear()`
  - 当前本地执行 `npm test` 时，绝大多数 suite 首个错误都是 `localStorage.clear is not a function` / `localStorage.getItem is not a function`
- 触发场景：
  - 任何本地或 CI 的 Vitest 执行
- 影响：
  - 前端回归保护几乎失效
  - 很多失败并不是业务回归，而是被测试基础设施噪音覆盖，难以及时发现真实问题
- 建议：
  - 在 setup 中先显式注入可用的 `localStorage/sessionStorage` mock 或做 capability guard
  - 再逐个清理已经和现状脱节的页面/API 测试
- 是否已有测试覆盖：有，但目前覆盖结果不可作为质量信号

### [P3] 更新检查源被硬编码到单一 GitHub 仓库，且失败结果会缓存 1 小时

- 分类：高风险硬编码
- 证据：
  - [src/routes/admin/monitoring.rs](/Users/liji/english/wordforge/src/routes/admin/monitoring.rs:57) 将更新缓存 TTL 固定为 3600 秒
  - [src/routes/admin/monitoring.rs](/Users/liji/english/wordforge/src/routes/admin/monitoring.rs:105) 把更新源固定写死为 `https://api.github.com/repos/Heartcoolman/wordforge/releases/latest`
  - [src/routes/admin/monitoring.rs](/Users/liji/english/wordforge/src/routes/admin/monitoring.rs:80) 在 GitHub 临时失败时也会把“无更新”结果缓存起来
- 触发场景：
  - 私有部署、fork 部署、离线部署，或 GitHub API 短时不可用
- 影响：
  - 管理后台会持续展示错误的“无更新”
  - 这类问题不是靠重试马上恢复，而是会被缓存掩盖 1 小时
- 建议：
  - 抽到配置项
  - 区分“成功缓存”和“失败缓存”，失败场景缩短 TTL
- 是否已有测试覆盖：仅有 `is_newer()` 的单测，没有覆盖真实远端配置与缓存策略

### [P3] Worker 调度和超时策略全部写死在代码里，缺少环境级调优入口

- 分类：环境配置项
- 证据：
  - [src/workers/mod.rs](/Users/liji/english/wordforge/src/workers/mod.rs:31) `WORKER_TIMEOUT` 固定 300 秒
  - [src/workers/mod.rs](/Users/liji/english/wordforge/src/workers/mod.rs:35) `DRAIN_TIMEOUT` 固定 30 秒
  - [src/workers/mod.rs](/Users/liji/english/wordforge/src/workers/mod.rs:115) 所有 worker 的 cron 都是源码常量
- 触发场景：
  - 不同部署规模、数据库性能、单机/多机角色切换、低资源环境
- 影响：
  - 运维层无法通过配置调低频率、错峰、延长超时
  - 只能改代码重新发版，放大了环境耦合
- 建议：
  - 把 cron/timeout 至少抽到 env 配置或系统设置
  - 保留当前常量作为默认值即可
- 是否已有测试覆盖：只有 `planned_jobs()` 是否注册的测试，没有环境调优覆盖

## 功能覆盖矩阵

| 功能域 | 入口索引 | 核心后端 | 前端触点 | 测试 / 文档 | 结论 |
|---|---|---|---|---|---|
| 认证 | `/api/auth`、`/api/admin/auth` | `src/routes/auth.rs`、`src/routes/admin/auth.rs`、sessions store | admin 登录页；用户侧 `authApi` 仅剩 refresh | `tests/auth_http.rs`、客户端指南 | 后端可用，用户侧 Web 客户端已残缺 |
| 用户与统计 | `/api/users` | `src/routes/users.rs`、users/records store | 当前成品无用户页 | `tests/users_http.rs`、旧前端测试 | API 基本正常，Web 页面资产缺失 |
| 单词/词本 | `/api/words`、`/api/wordbooks`、`/api/word-states` | words/wordbooks/word_states store | 当前成品无学习端 UI | `tests/words_http.rs`、`tests/coverage_routes_http.rs` | 后端主流程可跑，前端已不消费 |
| 学习主链路 | `/api/learning`、`/api/records`、`/api/study-config`、`/api/amas` | learning/records/study_config/AMAS | 当前成品无学习页 | `tests/acceptance_full_flow.rs`、`tests/amas_http.rs` | 后端主链路通过，Web 学习端未交付 |
| 用户画像 | `/api/user-profile/*` | `src/routes/user_profile.rs`、extras store | 类型和测试保留，但 API 文件缺失 | `tests/coverage_routes_http.rs`、OpenAPI | 存在真实契约错误与前端漂移 |
| 通知/内容增强 | `/api/notifications`、`/api/content` | `src/routes/notifications.rs`、`src/routes/content.rs` | 当前成品无对应页面 | `docs/api-reference/02-users.md` | 后端可访问，Web 消费端缺位 |
| 实时/SSE/遥测/状态 | `/api/realtime/events`、`/api/telemetry`、`/api/status`、`/health` | realtime/telemetry/status/health routes | `App.tsx`、`MonitoringPage.tsx`、telemetry worker | `tests/realtime_sse_http.rs`、状态相关文档 | 管理后台与全局状态链路已接入 |
| 管理后台 | `/api/admin/*`、`/admin/*` | admin routes、settings、analytics、monitoring | `frontend/src/pages/admin/*` | 后端 admin 测试、前端 build | 当前前端实际交付核心区域 |
| 后台任务 | worker scheduler | `src/workers/*` | 无直接页面，间接体现在后台/数据聚合 | worker tests | 调度可运行，但配置硬编码较重 |
| 文档/测试契约 | OpenAPI、Markdown、Vitest、Playwright | docs + tests | 整个前端目录 | `npm test`、E2E 规格 | 当前漂移最明显的横切问题 |

## 硬编码分类矩阵

| 类别 | 位置 | 用途 | 风险等级 | 建议动作 |
|---|---|---|---|---|
| 协议固定 | [src/routes/mod.rs](/Users/liji/english/wordforge/src/routes/mod.rs:30) | API 请求体上限 2 MiB | 低 | 保留但补文档 |
| 协议固定 | [src/main.rs](/Users/liji/english/wordforge/src/main.rs:21) | CSP / HSTS 安全头 | 低 | 保留但补文档 |
| 产品策略常量 | `src/amas/*`、`frontend/src/lib/constants.ts` | 学习阈值、提示时长、疲劳参数 | 中 | 抽到集中常量 |
| 环境配置项 | [src/config.rs](/Users/liji/english/wordforge/src/config.rs:174) | 默认密钥、端口、CORS 默认值 | 中 | 保留但补文档 |
| 环境配置项 | [src/workers/mod.rs](/Users/liji/english/wordforge/src/workers/mod.rs:115) | 全量 worker cron 与超时 | 高 | 抽到配置 |
| 测试专用 | [frontend/vitest.config.ts](/Users/liji/english/wordforge/frontend/vitest.config.ts:4) | 测试 API base | 低 | 保留但补文档 |
| 测试专用 | [frontend/e2e/auth.spec.ts](/Users/liji/english/wordforge/frontend/e2e/auth.spec.ts:5) | 旧学习端路由假设 | 中 | 拆为测试数据 |
| 高风险硬编码 | [src/routes/admin/monitoring.rs](/Users/liji/english/wordforge/src/routes/admin/monitoring.rs:105) | GitHub release 更新源 | 高 | 抽到配置 |
| 高风险硬编码 | [frontend/index.html](/Users/liji/english/wordforge/frontend/index.html:12) | `eng_theme` 存储 key 直写 | 中 | 抽到集中常量 |

## 验证记录

- 后端：执行 `cargo test`，大部分后端与集成测试通过，但 `tests/coverage_routes_http.rs` 失败，失败点为 `/api/user-profile/habit` 的返回字段不匹配。
- 前端构建：执行 `npm run build` 成功，说明当前交付物能构建出后台前端。
- 前端测试：执行 `npm test` 失败，大量 suite 首个错误集中在 `localStorage.clear is not a function`，测试基线当前不可用。

## 假设与说明

- 本报告基于当前工作区审计，工作树存在未提交改动，结论反映的是“此刻仓库状态”而不是某个已发布 tag。
- 本次没有做代码修复，只做事实核对和风险分级。
