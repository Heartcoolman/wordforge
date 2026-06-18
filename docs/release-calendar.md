# 发版日历与兼容窗口

本页登记四个关联仓库（后端 + admin、Web 学习端、iOS 客户端、Android 客户端）的发版节奏与 API 兼容窗口，供跨仓协作时参考。

---

## 本仓（wordforge · 后端 + admin）

### 发版节奏

| 阶段 | 节奏 | 说明 |
|---|---|---|
| **Beta** | 按需，无固定周期 | 每个功能迭代完成即发；tag 形如 `vX.Y.Z-beta.N`，release.yml 自动标 Pre-release |
| **Stable GA** | 无预设固定时间窗 | beta 内测充分 + 质量门达标后发；tag 形如 `vX.Y.Z`（无 `-`） |
| **Patch（bugfix）** | 72 小时内响应 P0 | 安全漏洞或数据损坏类问题强制快速发版 |

> **rc 通道已废弃**（v1.1.0-beta.1 follow-up 决策）。原 v1.0 时代的 rc.1/rc.2 + 7 天稳态观察期机制（`scripts/rc-observation/`）一并下线。tag-lint 现仅允许 `^v\d+\.\d+\.\d+-beta\.\d+$` 形式的 pre-release tag，alpha/rc 投递会被拦下。

### 历史发版记录（v0.x）

| 版本 | 发布日期 | 类型 | 摘要 |
|---|---|---|---|
| v0.1.0 | 2026-02-12 | stable | 全量功能首发 |
| v0.1.1 | 2026-02-14 | patch | 前端功能完善 |
| v0.1.2 | 2026-02-14 | patch | 审查修复 + 管理员密码重置 |
| v0.1.3 | 2026-02-14 | patch | 版本更新检查 + 全局中文化 |
| v0.1.4 | 2026-02-15 | patch | 性能优化 + AMAS 准确度提升 |
| v0.2.0 | 2026-03-24 | minor | AMAS v2 — 全新 DSR 架构 |
| v0.2.5 | 2026-04-09 | patch | 存储层重构 + AMAS 增强 |
| v0.2.6 | 2026-04-10 | patch | 服务端选词接口 + OpenAPI 规范页面 |
| v0.2.7 | 2026-04-11 | patch | SSE 重连 + 遥测 worker 修复 |
| v0.2.8 | 2026-04-14 | patch | 遥测增强 + 客户端管理 + 心跳看门狗 |
| v0.2.9 | 2026-04-15 | patch | 稳定性修复 |
| v0.3.0 | 2026-04-17 | minor | async runtime 防阻塞改造 |
| v0.3.1 | 2026-04-24 | patch | release.yml + install.sh |
| v0.3.2–3.4 | 2026-04-25–29 | patch | 稳定性修复 |
| v0.4.0 | 2026-05-02 | minor | admin 自更新 + AMAS 调参后台 |
| v0.4.1 | 2026-05-17 | patch | AMAS 11 维 Tier-A 调参 + 学习模式扩展 |
| v0.4.2 | 2026-05-17 | patch | worker cron 修复 + schema init 修复 |
| v0.4.3 | 2026-05-18 | patch | updater code review 修复 |
| v0.4.4 | 2026-05-18 | patch | updater 端口竞态修复 |
| v0.5.0–5.6 | 2026-05-19 | patch | admin 一键升级链路七连发 |
| v0.6.0-beta.1 | 2026-05-20 | pre | Probe REPL + UI 全量加固 |
| v0.6.0-beta.2 | 2026-05-20 | pre | feedback ErrorBoundary 修复 |
| v0.6.0-beta.3 | 2026-05-20 | pre | admin/updates 双通道 + prerelease 规则 |
| v0.6.0-beta.4 | 2026-05-20 | pre | Release Notes markdown 渲染 |
| v1.0.0 | 2026-05-22 | stable | GA 🎉 51 项 MUST 全完，详见 CHANGELOG |

### v1.1 计划发版

v1.1 采用 **beta 单通道**滚动发布：原计划的 rc.1/rc.2/rc.3 工作量已并入 v1.1.0-beta.1 一次性发布；后续 beta 充分内测后直接切 GA。

| 版本 | 计划发布日 | 类型 | 摘要 |
|---|---|---|---|
| v1.1.0-beta.1 | 2026-05-23（已发） | pre | P0 资源包热更 + GDPR 真流式 NDJSON / P1 领域事件总线 / P2 重构 + 性能 + 文档 + clippy 清零（合并原计划 rc.1+rc.2+rc.3） |
| v1.1.0 | 待定 | stable | beta.1 内测无 P0 后切 GA |

### API 兼容窗口（v1 稳定版承诺）

> 以下规则在 v1.0 GA 发布后正式生效。

| 等级 | 变更纪律 | 弃用公告窗口 |
|---|---|---|
| **v1 stable** | 禁止破坏性变更；新增字段必须可选 + default | ≥ 2 个 minor（约 6 个月） |
| **v1beta** | 可加新必填字段 / 改 enum；变更须出现在 release notes Breaking 段 | ≥ 1 个 minor |
| **v0 / internal** | 无承诺，随时可改 | — |

弃用端点会在 response 加 `Deprecation: <date>` + `Sunset: <date>` header（RFC 8594）。

#### 已明确的立即生效条款（v1.0 发布时）

| 端点 | 状态 | Sunset |
|---|---|---|
| `GET /api/v1/*` | v1.0 发布起返回 410 Gone；永久冻结至删除 | v1.0 + 12 个月 |
| `GET /api/admin/monitoring/check-update` | **保留为内部 admin 端点**（决议见下） | 不删除 |

> **check-update 弃用决议（v1.1.4，路径 A）**：原计划「v1.1 删除、由 `/admin/updates/*` 取代」**已撤销**。
> 实测两者语义不可无损互替——check-update 是带 TTL 缓存的**轻量只读版本探测**（Dashboard 顶栏更新角标、
> Monitoring 版本卡的活跃数据源），`/admin/updates/*` 是重型 apply/rollback/backup 自更新机。强删迁移收益
> 小于风险（删后 Dashboard 仅静默丢版本角标、Monitoring 走 `allSettled` 无感，属软退化，但价值损失实在）。
> 故**保留为内部 admin 端点**（非公开 v1 契约的一部分，无 sunset）。后续若仍要弃用，须先把 Dashboard
> 角标迁到 `updatesStatus().stable?.hasUpdate` 再议。

---

## wordforge-web（Web 学习端）

> 本表由 wordforge-web 仓库维护者填写。

| 版本 | 发布日期 | 最低后端版本要求 | 说明 |
|---|---|---|---|
| v1.3.0 | 2026-06-17 | v1.1.2-beta.4 | 遥测 `device.model` 上报补齐（T1 跨仓协同落地）；含 m038 设备四要素硬校验兼容 |
| v1.3.1 | 2026-06-17（本次） | v1.2.0-beta.14 | 资源包三端通用容器（content-slots + app-config）；与 server v1.2.0-beta.14 对齐 |

**后端 API 兼容要求**：wordforge-web 依赖 `/api/*` v1 stable 端点，升级后端前请确认 release notes 无 Breaking 变更。

#### ✅ 已完成：遥测 `device.model` 上报（v1.1.3 · T1）

> 登记日期：2026-06-02 ｜ 完成：三端遥测四要素（平台 / 版本 header + `device.timezone` / `device.model`）已于 web v1.3.0 / iOS v1.2.2 / Android v1.3.0（vc6）补齐上报并联调通过；对应后端 v1.2.0-beta.1 已完成兼容。T1 跨仓协同验收通过。

后端自 **v1.1.2-beta.4（迁移 m038）** 起，`POST /api/telemetry` 对设备四要素（平台 / 版本 header + `device.timezone` / `device.model` payload）做**上线即生效、不受 strict-mode 开关控制**的硬校验，缺任一即返回 4xx；其中 **`device.model` 为新增必填**，缺失直接 `400 MISSING_DEVICE_MODEL`。生产环境已是 beta.4，**wordforge-web 若未上报 `device.model` 其遥测会被静默拦截（断流）**。

协同项：

- **wordforge-web 需在遥测上报体 `payload.device` 中补 `model` 字段**（Web 端无真型号，可落 `browser on OS` 派生标识或 `web-admin` 占位，确保非空）；同时确认已携带 `x-device-platform` / `x-app-version` header 及 `device.timezone`。
- **约定最低后端版本**：含 m038 硬校验的最低后端版本为 **v1.1.2-beta.4**（及其后 GA v1.1.2 / v1.1.3）。wordforge-web 补齐上报后即可兼容该区间；旧后端（< beta.4）对这些字段宽容，向后兼容无碍。
- 契约细节见 `docs/api-spec.md` §11「遥测载体契约」。
- 本仓 admin-ui 自身遥测已于 v1.1.3 修复（`admin-ui/src/lib/device.ts` 补 `model`）；跨仓侧 web v1.3.0 / iOS v1.2.2 / Android v1.3.0（vc6）均已补齐上报并联调验收通过，本项 **已完成**。

##### 403 客户端降级处置矩阵（跨仓联调验收清单）

> 两个 403 码语义**不同，不可混为一谈**。`device.model` 等四要素补齐后，归属态仍可能返回 403；
> 客户端须按码差异化处置。⚠️ 这两个 403 是 m038 **故意硬拦截、无灰度开关**，客户端只能**降级**，
> **严禁要求后端放宽行为**。

| 403 码 | 含义 | 客户端处置 | 可否自动恢复 |
|---|---|---|---|
| `DEVICE_NOT_REGISTERED` | 设备未注册（无 `client_devices` 记录） | 引导用户正常登录使用——首个登录用户会 claim 该设备（owner 由 NULL 置为该用户），之后遥测自动放行。**可保留本地遥测队列**，恢复后重发。 | ✅ 可（首登 claim） |
| `DEVICE_OWNERSHIP_MISMATCH` | 设备已注册且归属**其他**账号 | **持续 403，须静默丢弃、不重试**（避免无意义重试放大写压力）。不弹错给用户，不入遥测重发队列。 | ❌ 否（须换设备标识或后端运维介入） |

**联调验收**：wordforge-web 补 `device.model` 后，分别构造「未注册设备」与「他人设备」两条上报，验证
客户端各自走上表处置（前者恢复、后者丢弃），即视为 T1 跨仓协同验收通过。

---

## iOS 客户端

> 本表由 iOS 客户端维护者填写。

| 版本 | TestFlight 发布日 | 最低后端版本要求 | App Store 状态 | 说明 |
|---|---|---|---|---|
| v1.1 | 待定 | v1.1.0-beta.1+ | TestFlight | 资源包热更联调 |
| v1.2.2 | 2026-06-16 | v1.1.2-beta.4 | TestFlight | 遥测设备四要素补齐；m038 硬校验兼容 |
| v1.2.3 | 2026-06-17（本次） | v1.2.0-beta.14 | TestFlight | 资源包三端通用容器（content-slots + app-config）；与 server v1.2.0-beta.14 对齐 |

**后端 API 兼容要求**：iOS 客户端仅调用 `/api/*` v1 stable 端点（不走 `/api/v1/*`），理论上兼容 v0.6.0+ 后端。strict-mode 启用时，客户端 User-Agent 必须符合 `WordForge-iOS/<semver>` 格式。

---

## Android 客户端

> 本表由 Android 客户端维护者填写。

| 版本 | versionCode | 发布日期 | 最低后端版本要求 | 说明 |
|---|---|---|---|---|
| v1.3.0 | 6 | 2026-06-17 | v1.1.2-beta.4 | 遥测设备四要素补齐；m038 硬校验兼容 |
| v1.3.1 | 7 | 2026-06-17（本次） | v1.2.0-beta.14 | 资源包三端通用容器（content-slots + app-config）；与 server v1.2.0-beta.14 对齐 |

**后端 API 兼容要求**：Android 客户端仅调用 `/api/*` v1 stable 端点。strict-mode 启用时，客户端 User-Agent 必须符合 `WordForge-Android/<semver>` 格式。

---

## 四端版本对应矩阵

> 同一行内各端版本经联调验证可协同工作；升级时建议整行对齐。

| 后端 | web | iOS | Android | 日期 | 备注 |
|---|---|---|---|---|---|
| v1.2.0-beta.12 | v1.3.0 | v1.2.2 | v1.3.0（vc6） | 2026-06-16 | 遥测设备四要素（m038）对齐基线 |
| v1.2.0-beta.13 | v1.3.0 | v1.2.2 | v1.3.0（vc6） | 2026-06-16 | 后端迭代，客户端沿用 |
| v1.2.0-beta.14 | v1.3.1 | v1.2.3 | v1.3.1（vc7） | 2026-06-17 | 资源包三端通用容器 content-slots + app-config |

---

## 跨仓发版协调流程

1. 后端发布 stable 版本 → 维护者在此页更新"历史发版记录"表
2. wordforge-web / iOS 维护者评估 release notes 中的 Breaking 变更：
   - 有 Breaking → 更新客户端并指定最低后端版本
   - 无 Breaking → 沿用现有最低版本
3. 有新的 API 弃用公告 → 各客户端在下一个 minor 版本内迁移到新端点
4. v1.0 GA 发布前，三方联合冒烟测试：后端 + Web + iOS 端到端流程全通
5. v1.1：iOS 联调需先发后端 v1.1.0-beta.1（资源包端点 + manifest + SSE 事件就绪，已于 2026-05-23 发布）

---

_本页最后更新：2026-06-17_
