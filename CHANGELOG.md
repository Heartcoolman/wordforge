# Changelog

所有版本变更记录均在此文件。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

后续每次发版在文件顶部手动 append 最新条目，再执行 `cp CHANGELOG.md docs/changelog.md` 同步站点副本（两份须保持一致）。注：本文件的手写复盘内容 GitHub Release body 不含，故**不可**用脚本从 Releases 全量重建覆盖。

**写作规范**（条目直接展示在 admin 升级页，受众是运维 / 管理员与用户）：
- 写「变了什么 · 对你的影响」，不写实现；**去除**函数名 / 文件路径 / 测试名 / 内部表名 / 根因分析 / commit·PR 细节（这些归 commit message 与 PR 描述）。
- **保留**运维必须感知的标识符：环境变量名、API 路径、配置项。
- 一条一行一个变化；分组用 新增 / 变更 / 修复 / 安全 / 移除 / 弃用；默认值或安全相关单列「安全」。
- 不写「测试」小节（CI 职责）。简体中文、精简、无营销形容词。

---

## [v1.2.0-beta.22] — 2026-06-20 · Beta · 内置托管学习端 Web + 管理界面迁至 /admin

### 新增
- 后端可内置托管学习端 Web 应用：管理员经资源包「稳定」通道上传 Web 工件后，站点根路径即时切换到该版本（原子切换，旧版本仍可回退）。未发布 Web 工件时根路径返回 404，不影响 `/api` 与 `/admin`。
- 系统监控新增一组只读遥测接口（`/api/admin/monitoring` 下：端点流量、定时任务的计划与运行历史、进程资源历史、请求分阶段管线指标）；本版仅接口就绪，对应监控大屏页面后续接入。

### 变更
- 管理界面访问地址由根路径 `/` 迁至 `/admin`（根路径让位给学习端 Web 应用，未发布 Web 工件时根路径返回 404）。请将管理后台书签、反向代理与放行规则更新到 `/admin`。

---

## [v1.2.0-beta.21] — 2026-06-19 · Beta · 监控大屏识别过载降级

### 新增
- 系统监控大屏现可识别「过载降级」：当请求延迟（P99）、在途请求、数据库写连接池或实时连接（SSE）接近容量上限时，后端健康状态标记为「降级·过载」，「后端可用性」卡同步提示降级原因——不再因仅统计 5xx 错误而在过载（请求变慢但未失败）时仍显示 100% 可用。

---

## [v1.2.0-beta.20] — 2026-06-19 · Beta · 修复监控大屏 P99 延迟虚高

### 修复
- 监控大屏「P99 延迟」虚高（常显示 1s 以上，而业务接口实际仅毫秒级）：`/health` 与监控页的健康检查不再同步等待词书中心（境外 CDN）的探活往返，改为后台定期探测；两个端点响应从数百毫秒~2s 降到毫秒级，P99 统计恢复真实水平，负载均衡器 / 探活脚本访问 `/health` 也同步加速。

---

## [v1.2.0-beta.19] — 2026-06-19 · Beta · 修复回滚到旧版本失败

### 修复
- 回滚到 v1.2.0-beta.11 之前的旧版本时，因旧版本不识别升级前的冒烟自检参数而被当成正常启动、抢占端口启动失败，导致回滚在换版本前中止；现回滚流程对老目标跳过该自检（完整性仍由 sha256 校验，可运行性由回滚后健康检查 + 自动回退兜底），可正常回滚到 v1.1.0 起的任意历史版本。失败的回滚在换版本前中止，原版本不受影响。
- 回滚到签名机制引入（v1.2.0-beta.11）之前的未签名旧版本时，不再被误判为「降级攻击」而阻断（升级路径仍严格要求签名）。

---

## [v1.2.0-beta.18] — 2026-06-18 · Beta · 重做回滚：任意版本可回退 + 数据可恢复

### 新增
- 升级页新增「回滚到任意版本」入口：可下拉选或手输任意历史版本回滚，不再局限于版本历史里最近两条。
- 回滚二次确认前新增预检 `POST /api/admin/updates/rollback/preflight`：展示目标与当前数据库版本、回滚后将不在新库中的「新增功能」清单、以及是否需先下载该版本确认。
- 回滚前自动对当前库做全量安全备份 `backup-pre-rollback-*`（永不自动清理），并显示在「备份」列表中；回滚丢弃的「升级后新写入数据」可随时从此备份恢复。

### 变更
- 回滚机制重做：回滚到旧版本时按目标版本把数据库结构降级，而非依赖预存快照，因此可回滚到任意已知历史版本（≥ v1.1.0）。目标版本之后才新增的功能数据不会进入回滚后的库（这些数据在该版本本就不存在，且已在上述安全备份中）；核心学习数据原地保留。
- 回滚到过旧（早于 v1.1.0）或未登记的版本会被拒绝并提示改用更近版本或「备份恢复」。

---

## [v1.2.0-beta.17] — 2026-06-18 · Beta · 监控大屏深度数据 + 移除全量导出接口

### 新增
- 监控大屏新增深度数据接口（管理员）：`GET /api/admin/dashboard/learning`（学习深度：答题响应时延分位、首答正确率、会话完成/放弃率、自评一致性、难点词分布、题型×难度、词库学习对比、掌握度分布、连续学习天数、分钟级活跃热力）与 `GET /api/admin/dashboard/amas`（AMAS 深度：A/B 实验分组对比、冷启动质量、ELO 趋势）。
- 大屏概览 `GET /api/admin/dashboard/overview` 健康区新增限流命中计数（按 用户/匿名/管理员/登录暴破 维度）。
- 后台任务执行状态新增异常（panic）计数。

### 移除
- 移除全量数据导出接口 `GET /api/admin/system/export`（beta.16 引入）：该接口为整库逐行导出、含账号凭证等敏感字段，已由监控大屏的聚合指标接口替代；如需整库备份请用 `POST /api/admin/updates/backup`。

### 变更
- 数据库迁移 m059（启动自动应用）：后台任务执行记录新增异常计数字段。

---

## [v1.2.0-beta.16] — 2026-06-18 · Beta · 全系统数据全量导出接口（仅超管）

### 新增
- 管理后台新增全系统数据导出接口 `GET /api/admin/system/export`（仅超级管理员）：以 NDJSON 流式导出全部数据表的所有记录，首行附完整表清单，用于数据迁移 / 离线分析 / 审计留档。

### 安全
- 该导出为原样全量，包含账号凭证哈希等敏感字段（不脱敏），仅超级管理员可调用且每次调用记录日志；导出文件等同全量凭证，请按机密妥善保管。

---

## [v1.2.0-beta.15] — 2026-06-18 · Beta · 升级页日志展示修复 + AMAS 自研文档补全

### 修复
- 管理后台「系统升级」页更新日志：原仅展示 commit 标题摘要（feat / fix / perf 三类），会漏掉以 CHANGELOG 正文记录的变更（如文档、运维类条目）；现优先展示 CHANGELOG 完整条目（新增 / 变更 / 修复等分节），commit 摘要降为兜底，并保留 commits 计数与 GitHub compare 链接。

### 变更
- 补全 AMAS 自研文档（反映 beta.14 算法升级）：「AMAS 入门」新增 beta.14 新增能力章节（记忆引擎三特征 FSRS-6 先验 + Tier-1 调度引擎：Parallel Elo / 动态 K / SSP 状态相关 DR / 留存 A/B，均默认全关需显式开启）；「调参管理后台」补各新特性的确切配置项、默认值与开启方式；本次 AMAS 进化调研 3 篇记录（调研终稿 / Tier 1-2 路线图 / 证据方法论）接入文档站导航。

---

## [v1.2.0-beta.14] — 2026-06-17 · Beta · AMAS 记忆引擎（默认全关）+ 文档站全面更新

### 新增
- AMAS 记忆引擎：新增 FSRS-6 三特征先验后端与冷启动先验，**默认全部关闭**，需在调参后台显式开启后才生效；新增相关配置项与默认值，并导出至 admin schema。
- AMAS 留存 A/B 验证闭环地基：实验分桶与单词掌握度事件记录，用于真实留存对照。

### 变更
- 数据库迁移 m056–m058（启动自动应用）：新增 A/B 实验相关表、ELO 动态 K 趋势与选词链路字段。
- 文档站全面更新并补齐到四端现状：新增「资源包热更」专题（机制、两个通用容器 content-slots / app-config、三端消费、deeplink 契约、admin 操作、安全约束）、Android 客户端安装文档、资源包运维 Runbook；API 文档补设备管理（`/api/admin/clients`）与资源包安装遥测端点；指南补资源包热更架构与四端说明；发版日历补四端版本矩阵。

---

## [v1.2.0-beta.13] — 2026-06-17 · Beta · 修复资源包上传失败

### 修复
- 资源包上传新版报「服务器内部错误」(500)：此前上传的 payload 无法写入服务器磁盘，导致任何资源包都传不上去；现已修复，上传后可正常签名落盘，并切通道激活下发到客户端。

---

## [v1.2.0-beta.12] — 2026-06-17 · Beta · Web 设备指纹封禁 + 关联风控复核

### 新增
- 设备管理新增「风险标记」复核标签页：列出因封禁被关联牵连的设备，可逐台复核、解除标记或直接封禁（API：`GET /api/admin/clients/flagged`、`POST /api/admin/clients/:id/clear-flag`）。
- 封禁设备时返回被关联牵连的设备数量，操作后提示有多少台共享 IP / 账号 / 设备指纹的设备进入复核队列。
- 设备列表与设备详情对被牵连设备显示「风险」标记及标记原因、触发源、标记时间。

### 安全
- Web 端设备封禁升级为浏览器指纹封禁：清缓存、隐私模式、换标签页换取的新设备标识无法再绕过封禁（强指纹精确命中直接拒绝访问）。
- 同一硬件换浏览器（仅低熵指纹相同）不直接封禁，改为打风险标记进复核队列，避免误伤。
- 封禁某设备时，自动给共享出口 IP / 同账号 / 同硬件指纹的其它设备打风险标记（仅标记不自动封禁，避免共享网络误伤）；解封触发源设备时同步清除其牵连出的全部标记。
- 说明：指纹封禁对专门的反检测 / 指纹浏览器无效，属 Web 端固有上限。

### 变更
- 设备存活判定与遥测采样解耦：合法上报即刷新设备心跳，不再受 `telemetry_sample_rate` 影响（此前采样率 < 1.0 时被丢弃的周期心跳会被漏记）。

### 修复
- 修复设备数据状态面板对历史设备恒显示「未上传」的问题：存量设备无需重新答题即正确转为「已上传」。
- 修复部分引擎监控记录的奖励信号字段写入错误的问题。
- 启动自检移除对内部版本号为空的误报校验项。

## [v1.2.0-beta.11] — 2026-06-17 · Beta · 升级自检加固 + 真实分步进度

### 新增
- 升级进度改为真实分步流水线动画：下载 → 校验完整性 → 校验签名 → 解压 → 新版本自检 → 备份数据库 → 替换 → 重启 → 确认新版本健康，每一步对应实际进展。
- 替换二进制前先试运行新版本做兼容性自检，不兼容 / 损坏的安装包在动到现有服务之前即被拦截，本机服务保持不变。

### 修复
- 网络较慢时升级下载不再被固定时限中断（下载时限按安装包大小自动放宽）。
- 升级后的健康检测在精简 / 容器化部署环境下也能稳定工作，不再无谓触发自动回滚。
- 回滚目标版本若启动失败，数据库会与程序一起回退到回滚前状态，不再出现程序与数据不一致。

## [v1.2.0-beta.10] — 2026-06-17 · Beta · 升级健康判定更准

### 修复
- 升级后的健康监督现在能真正识别「新版本起来了但数据库不可用」这类坏升级并自动回滚（此前只看进程是否在服务、会漏判）；维护模式下仍不误判新版本不健康。

## [v1.2.0-beta.9] — 2026-06-17 · Beta · 修复升级总被回滚

### 修复
- 修复「升级总是失败并自动回滚」的回归（beta.7 引入）：自更新后新版本被监督进程误判为不健康而回滚，导致从 beta.7 / beta.8 起任何升级都无法成功。现在升级可正常完成。
- 升级期间的健康监督改用存活探针，维护模式开启时不再把刚起来的新版本误判为不健康。

> 注：从 beta.7 / beta.8 升级到本版本需由运维直接替换二进制（旧版本的升级流程有上述缺陷，无法自更新到本版本）。

## [v1.2.0-beta.8] — 2026-06-17 · Beta · 回滚加固:数据库一致 + 后台永不挂

### 变更
- 版本回滚现在会**连同数据库一起恢复到目标版本的快照**，保证回滚后程序与数据严格一致，不再出现旧版本撞上新数据结构而反复重启。注意：升级到当前版本之后写入的数据会随回滚一并丢失（回滚前会自动备份当前库以便复原）。
- 仅可回滚到「有本地数据库备份的近期版本」；升级页的回滚入口据此收敛，回滚到过旧 / 不兼容版本会被直接拒绝并提示可回滚目标。

### 修复
- 回滚 / 自动回滚不再可能导致管理后台打不开：修复了极端情况下界面资源缺失造成的「Not Found」；服务启动时若检测到界面资源缺失会自动从备份恢复。

## [v1.2.0-beta.7] — 2026-06-16 · Beta · 升级历史收尾 + 自更新回滚安全网

### 修复
- 升级历史不再把记录长期显示为「进行中」：服务重启后会把残留的升级 / 回滚中间状态正确收尾为「成功」（修复了前两版未覆盖到的状态，历史记录恢复正常显示）。
- 恢复自更新的失败自动回滚安全网：升级后新版本若 60 秒内健康检查持续不通过，自动回滚到上一版本并记为「已回滚」（此前监督进程会在重启时被系统提前终止而失效）。

### 变更
- 既有部署需为 systemd 服务设置 `KillMode=process` 并执行 `systemctl daemon-reload`，上面的回滚监督才能跨重启存活生效；全新安装已默认包含。

## [v1.2.0-beta.6] — 2026-06-16 · Beta · 升级页显示与审计修复

### 修复
- 升级页「最新 稳定」不再把当前 Beta 版本误显示为稳定通道的最新版；通道暂无可用版本时显示「—」/「暂无可用版本」。持续发布 Beta 期间也能稳定识别上一个稳定版（更新检查窗口加大）。
- 升级历史不再把记录长期显示为「进行中」：自更新成功重启后，审计记录现能正确收尾为「成功」（修正了重启后写审计库路径错配的问题）。

## [v1.2.0-beta.5] — 2026-06-16 · Beta · 系统设置真生效 + 升级重启体验修复

### 新增
- 系统设置「配置板块」新增「运行时 · 即时生效」分组，可在线调整并**即时生效、无需重启**：速率限制（窗口与匿名 / 登录 / 认证 / 遥测各档配额）、请求配额（批量条数，导入 / 拉取 / 统计上限，SSE 连接上限，分页大小，限流器容量），认证令牌有效期（访问 / 刷新 / 管理员）。改动持久化，重启后保留。

### 修复
- 升级 / 回滚后服务重启期间，管理页自动重连并刷新到新版本，不再停留在「Bad Gateway」。
- 升级历史不再把因重启来不及收尾的记录长期显示为「进行中」，启动时自动收尾为成功。
- 升级页「安装于」时间按通道展示：仅在当前运行版本所属通道（稳定 / Beta）下显示，避免两通道显示相同时间。

## [v1.2.0-beta.4] — 2026-06-16 · Beta · 清理虚假功能 + 对齐修复

### 移除
- 反馈回复的「抄送邮箱」开关（无发邮件能力）。
- 系统设置清理仅存配置、当前不生效的板块（站点 / 速率限制 / 审计 / 邮件 / 短信 / 存储等），只保留真正生效的「认证与登录」「备份策略」；维护模式 / 版本门控 / 管理员 / API 密钥 / 配置快照不受影响。
- 设备升级策略的「灰度比例」「PWA 静默」、AMAS 灰度的「24h 自动扩量」、数据分析的「SQL 预览」——均无实际后端行为。

### 修复
- 数据分析「增长漏斗」各行数值列对齐。
- AMAS 影响评估副标题改为如实的「参数变化启发式估计」（此前误标「遥测回归」）。
- 顶栏账户区不再呈现为可点击按钮、不再硬编码角色文案。

## [v1.2.0-beta.3] — 2026-06-16 · Beta · 远程探针默认启用 + 多项设计对齐

### 安全
- 远程探针默认启用（`PROBE_ENABLED` 默认 `true`）；设 `PROBE_ENABLED=false` 可关闭。仍受限速、审计留痕、沙箱白名单与脱敏约束。

### 新增
- 远程探针脚本模板 +6：健康总览 / 控制台日志 / 页面加载时序 / 网络质量 / 时钟时区 / 应用状态快照。

### 变更
- 新功能导览改版为 v1.2.0 重设计内容（5 屏）；升级后对老用户重新弹出一次。

### 修复
- 算法决策分布：副标题改为「N 种算法占比」，无数据时显示空状态。
- 升级历史仅展示升级 / 回滚，不再混入封禁 / 改密等其它管理操作。
- 升级页 CHANGELOG 按章节渲染（此前显示为 markdown 源码）。

## [v1.2.0-beta.2] — 2026-06-16 · Beta · 管理后台蓝玻璃重设计 + 自动刷新滚动修复

后端内嵌管理 GUI（admin-ui，SolidJS）整体重做为蓝玻璃（glassmorphism）设计：全部 16 个路由页 + 外壳重新换肤与排版，沿用既有真实 `adminApi`。本次为**纯前端（运维面板）发布**，服务端核心无逻辑变更，仅版本号 bump（core 1.2.0-beta.1 → 1.2.0-beta.2），无破坏性变更。

### 管理后台重设计
- 全新设计系统 `admin-ui/src/styles/wf-admin.css`，**作用域限定 `.wf-admin` 根**——保留的登录/初始化页维持原 OKLCH/Tailwind 外观；暗色模式复用既有 `.dark` 类（非原型的 `[data-theme]`）。
- 新增组件层 `admin-ui/src/components/wf/`：`Icon` / `primitives`（Btn/Card/Badge/Modal/Drawer/StatCard/Panel/Tabs…）/ `charts`（手写 SVG Sparkline/LineChart/BarChart/Donut/Heatmap/Scatter）/ `format` / `sx()`（React 驼峰内联样式 → Solid kebab CSS 转换）。
- `AdminLayout` 重写为 7 组信息架构 + 玻璃侧栏/顶栏 + 动态 mesh 背景，复用既有 CommandPalette / NotificationBell / OnboardingTour / 主题与 token 逻辑；导航项与既有 `/admin/*` 路由 1:1 对应。
- 16 个页面改为自包含；图表移除 echarts、AMAS 配置编辑器移除 CodeMirror（改 textarea）→ 两个大 vendor chunk 从 admin 包移除，构建体积下降。

### 修复
- **自动刷新页滚动归顶**：5s 轮询页（仪表盘 / 设备 / 探针指标 / 版本更新）每次刷新滚动跳回顶部。根因为每个 admin 页包在页级 `<Suspense fallback={<PageSpinner/>}>` 下，`createResource` 的 `refetch()` 被追踪读取时会重新触发 fallback → 整页重挂载 → 滚动归零。修复：轮询/手动 refetch 包入 `startTransition`（transition 分支不递增 suspense 计数、保留旧内容直至新数据就绪），并将约 45 处 `<Show when={!res.loading}>` 局部加载门控改为 `res.state !== 'pending'`（refetch 期间状态为 `refreshing`、保留旧值，面板不再拆除重建）。

### 测试
- admin-ui：`tsc --noEmit` 0 错误、`vitest` 149 文件 / 1435 通过 / 9 跳过、`build` 绿。29 个受影响测试文件随新 UI 重写，删除死代码 `pages/amas-advisor/HistoryTable.tsx` 及其测试。

## [v1.2.0-beta.1] — 2026-06-15 · Beta · 三仓联调 + 版本统一

服务端 × iOS × Android 三仓联调收口：以服务端契约为准补齐客户端缺失功能与遥测，三端 marketing 版本统一为 **1.2.0**（服务端 core 1.2.0，保留 `-beta.1` 发布通道标记）。本条目记录客户端侧补齐，服务端仅版本号变更（核心 1.1.5-beta.1 → 1.2.0-beta.1），无破坏性变更。

### 版本统一
- 三端 marketing 版本统一为 **1.2.0**：服务端 `Cargo.toml` → `1.2.0-beta.1`（core 1.2.0 + beta 通道；`docs/openapi.yaml` 同步重生成）；iOS `MARKETING_VERSION` 1.2 → 1.2.0、build 2 → 3；Android `appVersionName` 0.2.0 → 1.2.0、`versionCode` 2 → 3。均满足后端 strict-mode User-Agent 的 3 段 semver 约束。

### 客户端遥测补齐（以服务端摄取 schema 为准）
- Android 补齐 `payload.featureUsage`、`behavior.{clickCount,routeChanges,visibilityChanges,scrollDepthPct,clickTargets}`；session 指标（actionsPerMin/errorCount/avgResponseTimeMs）改 nullable 省略，不再发送伪零；`device.{geoLat,geoLng,geoCountry}` 仅在定位已授权时填充、永不弹窗。
- Android 学习记录补齐 7 项 AMAS 行为信号（dwellTimeMs / pauseCount / switchCount / retryCount / focusLossDurationMs / interactionDensity / pausedTimeMs），与 iOS 对齐喂入 AMAS。
- Android 崩溃/错误上报：上传前对 message/stack 做 PII 与密钥脱敏、`url` 由线程名改为当前路由、新增 60s 去重环与非致命错误镜像通道。
- Android probe 结果 DTO 补齐 `resultJson` / `confirmToken`（契约对齐；真实 JS 沙箱执行延后，保持 `unsupported_ctx_version` 语义）。

### 客户端功能补齐（以 iOS 为参照达成跨端对等）
- Android 新增：主屏 Glance 小组件、应用快捷方式、学习日历热力图、收藏夹独立页、学习数据 CSV/JSON 导出、首启合规同意门、root/调试器检测遥测、OkHttp 证书绑定（明文 HTTP 期空 pin）、离线队列同步 UI。
- iOS 新增：quick-practice AMAS 校准热身屏、独立 session 反馈/总结屏。

### 客户端契约
- iOS `SyncProgressRequest` / `CompleteSessionRequest` 补齐 `events: [SessionEvent]`，经服务端 session 事件幂等摄取路径上报作答事件（与 Android 对齐）。

## [v1.1.5-beta.1] — 2026-06-15 · Beta · 遥测安全加固 + AMAS 算法整合

首个 1.1.5 beta，整合 v1.1.4 以来的安全修复与 AMAS 算法研究成果。

### 遥测系统安全修复（36 项确认缺陷，经 37-agent 对抗复验）
- **P0** SSE 连接计数泄漏：CAS 自增后立即建 RAII guard，任何 `.await` 取消点对称释放，杜绝累积到上限后全站 SSE 拒服。
- **P1** SSE per-user 配额 + 全局上限；设备总览 SUM 整数溢出改饱和聚合；离线设备 batch 致 admin SSE 挂起/broadcast 泄漏修复；遥测限频前置于设备写。
- **P1** telemetry 两表保留期清理 worker（`TELEMETRY_RETENTION_DAYS`，默认 90 天）。
- **P2** 告警分页（翻页覆盖全部未读）+ severity 入小时桶 dedup；采样规则 PATCH 白名单；`consume_confirm` 原子消费 + 纵深守卫；设备抢注格式校验 + 首占审计。

### AMAS 算法
- difficulty 缺值/默认 0.0 回落 no-op，消除 difflogit 对复习词的恒定 logit 偏置。
- 移除已证伪的 IAD/MTP 辅助模块。
- 含 GSP 毕业制调度头、difflogit/predLogit 预测读出层、benchmark 镜像。
- 对抗性审查 50 项缺陷修复 + 7 个系统性根因收敛，及其衍生 9 项收口。

### 客户端契约
- 三仓联调（服务端 × 安卓 × iOS）服务端 3 项契约修复（A10/A11/A26）。

### ⚠️ 生产语义变更（Beta 重点验证项）
- GSP 区间帽 90→40 天（实际调度间隔行为改变）。
- difflogit β=0.1 生产激活（依赖 `word.difficulty` 真实填充，当前默认 0.0）。
- IAD/MTP 模块移除。

## [v1.1.4] — 2026-06-05 · Stable · 1.1.x Admin 运维面转正

将经 Beta 阶段（v1.1.2-beta.1 ~ v1.1.4-beta.3）验证的 **1.1.x Admin 运维面大版本**转正为正式稳定版。这是自 v1.1.1 以来首个稳定版。

- **内容与 `v1.1.4-beta.3` 完全一致**，仅版本号变更（`1.1.4-beta.3` → `1.1.4`）；无新增功能、无破坏性变更。
- **累积转正范围**：Admin 运维面大版本（探针遥测看板 · AMAS 多块看板与灰度 · 词书中心 · RBAC · 反馈工单中心）+ 事件总线深化（幂等账本 · outbox 死信运维闭环）· 运营闭环（canary 自动回滚进告警收件箱 · 定时广播队列查看/取消 · 离站备份可观测）· 遥测硬识别与专项限频，以及一批安全/性能加固。逐项明细见下方各 Beta 条目。

### ⚠️ 升级前必读（从 v1.1.1 稳定版升级务必逐条确认）

相较 v1.1.1，本次累积 **数据库迁移 m022–m049、约 90+ 新增接口、Admin 界面大面积重构**，变更面非常广：

1. **先手动备份数据库再升级**：Admin「版本更新 → 备份」手动触发一次，或 SSH 备份 `data/learning.db`；升级后每日自动备份会跳过首个周期。
2. **迁移单向不可逆**：升级成功后库结构前滚至最新，无法在保留新数据的前提下干净回退。回退仅允许换回旧二进制（旧代码忽略新增列/表），**禁止**恢复旧库备份。
3. **当前运行版本须 ≥ v1.1.0-beta.3 才能一键升级**：更早版本旧升级器存在自重启死锁，需手动替换二进制。
4. **升级失败自动回滚只还原程序与前端，不还原数据库**：本批迁移纯增量，旧程序可在已前滚的库上继续运行。

---

## [v1.1.4-beta.3] — 2026-06-03 · Pre-release · 遥测面板分类 + 设备操作概览

### ✨ 新功能与改进

- **设备遥测面板重构为「操作概览 + 分类明细」**：设备管理「历史」面板顶部新增「这台设备做了什么」概览——功能使用排行 / 访问页面 / 点击热点 / 错误与事件分布 / 总时长与会话数，全部由后端全量聚合（featureUsage 累加、currentRoute 分组、clickTargets 按 label 聚合），一眼看懂无需逐条翻。设备画像提顶只显一次；原始记录折叠为明细，展开后系统按 event_type 分类筛选、点行看完整会话/行为/功能详情。新增 `GET /api/admin/telemetry/:device_id/summary` 聚合端点 + `event_type` 过滤。
- **新功能引导导览加入「设备操作概览」屏**：升级后导览新增一屏介绍设备遥测操作概览；v1.1.4 波次标识翻新（`v1.1.4r2`），已看过 v1.1.4 早期导览的人会再弹一次完整导览。

### 🐛 修复与改进

- **实时事件流 5 秒刷新不跳不闪**：数据探针页实时遥测流轮询刷新时，正在阅读会被滚动归零、spinner 闪烁。改为渲染快照与 resource 解耦、按 id 复用未变行 DOM、spinner 仅首帧、手动锚定滚动补偿（贴顶看最新、滚下去看历史锚点不动），并关掉浏览器原生 overflow-anchor 防双重补偿。

## [v1.1.4-beta.2] — 2026-06-03 · Pre-release · 引导导览换 v1.1.4 内容

> 在 v1.1.4-beta.1（16 项 backlog 全集，见下方条目）之上仅叠加一项 admin UI 修正，其余一致。

### 🐛 修复与改进

- **新功能引导导览换为 v1.1.4 内容**：升级后弹出的导览改为本版运营闭环新功能（事件 outbox 死信运维 · 人工重投 / 丢弃 · 定时广播队列查看与取消 · 离站备份可观测 · 告警收件箱 canary 自动回滚进箱 + 一键全部已读）；`waveOf` 为 v1.1.4 设独立波次，从 v1.1.3 升级到本版后自动重弹一次。

## [v1.1.4-beta.1] — 2026-06-03 · Pre-release · 收尾型 minor：事件总线深化 · 运营闭环 · 性能加固 · 契约对齐

把 v1.1.3 埋下的四块「基建已建、闭环未做」半成品（outbox / 告警收件箱 / 定时广播 / 离站备份）收口，并对 v1.1.x 累积的契约文档漂移止血。**全程无 P0**，生产硬校验正确生效、无线上断流。新增迁移 m045–m047。

### ✨ 新增与闭环

- **领域事件幂等账本（W1-1）**：新增 `processed_events` 表（m045），把幂等标记与 AMAS 状态在同一事务原子写入，single/batch 入口在应用 AMAS 前预检命中即短路，把「重启不丢」补成 AMAS「精确一次」——为后续删手动 rollback / 切默认异步铺第一块基石。**`RECORDS_OUTBOX_ASYNC` 默认仍 false（同步老路），手动 rollback 保留**，生产零暴露。
- **outbox 死信运维闭环（W1-2）**：admin 监控页死信 chip 可点开抽屉，列出明细（用户 / 事件类型 / 失败原因 / 进死信时间），支持人工重投（回 outbox、attempts 归零）与丢弃（均带二次确认）；毒丸消息（payload 解析失败 / 未知事件类型）判为永久错误，跳过指数退避直接进死信，不再空跑 5 次。
- **canary 自动回滚进告警收件箱（W2-1）**：AMAS patch canary 触发自动回滚时写入 admin 告警收件箱（持久可追溯，含被回滚 patch 的 reward/anomaly 对比），补齐此前只发瞬态 SSE 的覆盖盲区。
- **定时广播队列查看 / 取消（W2-2）**：DevicesPage 新增「待发排程」队列视图，可查看与取消待发的定时广播（原子抢占：已被到点下发则返回 409 不误取消）；迁移 m046 重建 `scheduled_broadcasts` 表加入 `canceled` 状态。
- **告警收件箱「全部已读」（W2-3）**：NotificationBell 面板头一键清空未读角标。
- **离站备份可观测（W2-4）**：`backup_offsite` worker 落 worker 心跳（admin worker 列表 + Prometheus 可见）；新增 `backup_target_status` 表（m047）记录每 target 上次成功 / 失败 / 字节数，settings 备份面板按 target 显示状态，灾备从「配了不知有没有用」变为可验证。
- **遥测专项限频（W3-1）**：`/api/telemetry` 增加独立于通用 API 预算的 per-user 频率配额（`RATE_LIMIT_TELEMETRY_MAX`，默认 120/60s），超额软丢弃（不落库、仍刷新设备活跃度），防单个噪声客户端打满 SQLite 写路径挤占学习数据写入。
- **门控变更审计留痕（W4-4）**：客户端最低版本门控（高危生产控制）变更时写入 `update_audit_log`，记录 old→new 与开关翻转。

### 🐛 修复与加固

- **关停最终落盘（W3-2）**：graceful shutdown 在服务停止后补一次可用率落盘，消除重启丢失登录 SLO 当前小时桶（≤5min）的缺口。
- **延迟直方图桶细化（W3-3）**：`http_request_duration_seconds` 桶边界细化为 `[0.01,0.025,0.05,0.1,0.25,0.5,1.0,2.5]`，提升 100ms~2s 区间 p95/p99 精度；启动回灌对桶数变更的旧持久化行做安全重置（防 resize 错位污染历史 SLO，详见 `docs/runbook/metrics-buckets.md`）。

### 📝 文档与契约

- `docs/openapi.yaml` `info.version` 改由 `CARGO_PKG_VERSION` 派生，跨 v1.1.x 不再漂移（W4-1）。
- `api-endpoints.md` 遥测段补齐 m038 四要素硬校验（平台 / 版本 header + `device.timezone` / `device.model`）+ 两个 403 三态归属表（W4-2）；用户段补齐 GDPR 注销（`DELETE /api/users/me`）与数据导出（`GET /api/users/me/export`，NDJSON / 24h 冷却 / 429）两端点（W4-3）。
- `release-calendar.md` / `RFC.md`：撤销 `check-update` 端点「v1.1 删除」承诺，明确保留为内部 admin 端点（W4-5）；新增按 403 码差异化的客户端降级处置矩阵（W4-6）。

### 🧹 代码债

- 删除 admin-ui 死模块 `api/health.ts`（全站零引用）及其测试；后端占位字段注释澄清语义（W5-1）。

> 质量：后端 `cargo test` 全绿、`clippy` 零新增告警；admin-ui `tsc` / `build` / `vitest`（1008 通过）全绿。11-agent 对抗式交叉验证捕获并修复 1 个 W1-1 原子性 blocker（回滚 AMAS 状态与清除幂等标记收敛为单事务，守住「标记存在 ⟺ AMAS 已应用」不变式）。

## [v1.1.3-beta.2] — 2026-06-03 · Pre-release · 引导导览与更新页 CHANGELOG 修正

> 在 v1.1.3-beta.1（19 项 backlog 全集，见下方条目）之上仅叠加两项 admin UI 修正，其余一致。

### 🐛 修复与改进

- **新功能引导导览换为 v1.1.3 内容**：更新后弹出的导览改为本版运维闭环新功能（告警收件箱 / 设备推送定时调度 + 草稿 / 客户端版本门控 / 事件 outbox 异步可观测）；`waveOf` 为 v1.1.3 设独立波次，升级到本版后自动重弹一次。
- **更新页 CHANGELOG 卡片展示完整更新日志**：有 release notes 时优先渲染完整富文本（覆盖全部变更），原 compare-commits 派生列表（仅显示区间内少量提交）降为无 notes 时的回退。

---

## [v1.1.3-beta.1] — 2026-06-02 · Pre-release · 遥测契约协同 + 性能运维加固 + 事件 outbox + 功能补全

收纳 v1.1.3 backlog 全 19 项（W1–W5）。

### ✨ 新增与变更

- **遥测契约协同（W1）**：admin-ui 自身遥测补 `device.model`（修 beta.4 硬识别引入的本仓自伤 400）；`tests/telemetry_http.rs` 补五拒绝码负路径断言；对齐 `docs/api-spec.md` / `docs/v1-client-migration.md` 过期契约。**顺带修复生产回归**：`device.rs` 遥测路径匹配 `/api/telemetry`→`/telemetry`（axum nest 剥前缀，beta.4 起遥测三态归属核验被旁路，本版修复后生效）。
- **性能运维加固（W2）**：SQLite 连接池默认 16→8（配合 beta.3 cache_size 收紧峰值内存上界）；nginx 两份样例补 named upstream + `keepalive 64` + `map $http_upgrade`；新增 `deploy/sysctl.d` 抗突发参数 + runbook「内核 / nginx」段；systemd `MALLOC_ARENA_MAX=2`。
- **DB 备份外迁（W2 · B1）**：每日备份按 `BackupTarget.uri` scheme 分发上传 file / rsync / S3（`object_store`），失败告警入 `system_alerts`，runbook 补离站章节。
- **领域事件 outbox 基建（W3 · S2-1）**：新增 `outbox` + `events_dead_letter` 表 + 异步消费 worker（指数退避重试 + 死信兜底）+ admin 监控 outbox lag / 死信展示 + 开关 `RECORDS_OUTBOX_ASYNC`。**默认 false 走同步老路（amas_result 同步返回、行为不变）**，异步路径 opt-in。⚠️ 异步模式响应不含 amas_result，且崩溃窗口下事件可能被 AMAS 重复应用（RFC R06，「重启不丢」≠「重启不重复」）；切默认 / 删手动 rollback 待与学习端跨仓协同。
- **功能补全（W4）**：admin 应用内通知 / 告警收件箱（`system_alerts` 加 read/ack + 收件箱组件 + 未读角标）；设备推送投递时机调度（延时 / 指定时间）+ 草稿存储；登录页 SLO 30d 经 `availability_rollup` 持久化跨重启可达；`min_client_version` 版本门控 admin 运行时可配（即时生效）。
- **前端配套 + 代码债（W5）**：广播受众 400 码（`INVALID_VERSION_MIN` / `EMPTY_AUDIENCE`）前端提示 + semver 预校验；广播历史 week/failed 筛选下推后端跨页；canary 自动回滚阈值迁 `system_settings` 可配；vite manualChunks 拆 `vendor-echarts` / `vendor-codemirror`；v1 弃用文档 URL 改真实地址 + 稳定锚点；移除登录页 i18n 占位控件；删除失效的 `build-changelog.sh`。

### 🗄️ 迁移

- `m039` availability_rollup · `m040` canary 阈值 · `m041` system_alerts 收件箱列 · `m042` scheduled_broadcasts + push_drafts · `m043` min_client_version 门控 · `m044` outbox + events_dead_letter

### 📌 勘误

- 修正 rc.2 条目对「领域事件总线」的夸大表述（彼时仅落基础设施 + 计数旁路 consumer，非 AMAS 异步消费）。

### 🧪 质量

- 后端 `cargo test` 全过（含 outbox 重启不丢 / 死信、遥测五拒绝码、迁移幂等）；admin-ui `tsc` / `build` 通过、vitest 137 文件全过；24-agent 对抗式交叉验证 0 blocker，确认问题已修复或文档化。

---

## [v1.1.2-beta.4] — 2026-06-02 · Pre-release · 遥测硬识别 + AMAS 数据软拦截告警

在 v1.1.2-beta.3 基础上叠加遥测身份强制与 AMAS 数据失败告警，其余一致。

### ✨ 新增与变更

- **遥测硬识别（破坏性契约变更，无开关）**：`POST /api/telemetry` 强制四要素——平台（`x-device-platform`）、版本（`x-app-version`）、时区（`payload.device.timezone`）、设备型号（`payload.device.model`，**新增必填字段**），缺任一直接 4xx；并校验设备已注册且归属一致（盗用 / 伪造 device_id → 403），归属为空时由首个登录用户认领。**需客户端配合上报 `device.model` 等字段，否则遥测被拦**
- **AMAS 数据软拦截告警**：`metrics_flush` / `daily_aggregation` worker 与学习记录上报失败不再静默吞掉——失败不阻断流程，但主动告警。新增 `system_alerts` 表经 `/admin/monitoring` 监控时间线透出（admin 运维），受影响用户收应用内通知（小时桶去重防风暴）
- **设备型号落库展示**：设备列表新增「型号」列、详情抽屉展示型号、CSV 导出含型号；版本 / 型号分布「未知」改「未上报」

### 🗄️ 迁移

- `m037` 新增 `system_alerts` 表；`m038` `client_devices` 新增 `model` 列

---

## [v1.1.2-beta.3] — 2026-06-01 · Pre-release · 系统监控过载饱和信号 + SQLite 防 OOM 收紧

在 v1.1.2-beta.2 基础上叠加两项运维加固，其余一致。

### 🐛 修复与性能

- **系统监控新增过载饱和信号**：补 `http_inflight_requests`（in-flight gauge），修复「5xx=0 即显健康」盲区——请求堆积但未报 5xx 的过载状态现可观测、可告警
- **收紧 SQLite 每连接 `cache_size` / `mmap_size`**：`cache_size` -64000 → -16000（≈62.5 MiB → 15.6 MiB/连接）、`mmap_size` 256 MiB → 128 MiB，按 pool 连接独占口径压低峰值内存，防高并发 OOM

---

## [v1.1.2-beta.2] — 2026-05-31 · Pre-release · 新增大版本新功能引导导览

在 v1.1.2-beta.1 基础上**仅新增「新功能引导导览」**，其余内容（约 90 新增接口 / 15 迁移 / Admin 多块看板与安全性能修复）与 beta.1 完全一致，详见下方 [v1.1.2-beta.1]。

- 升级进入本大版本后自动弹出一次全屏分步导览，逐屏介绍各新增看板并可一键直达
- 同一大版本波次内重复升级不再重弹；顶栏「导览」按钮随时可重看；支持键盘 ←/→/Esc 操作

---

## [v1.1.1] — 2026-05-31 · Stable · v1.1.0-beta.4 转正

将经 Beta 阶段（v1.1.0-beta.1 ~ beta.4）验证的内容**转正为正式稳定版**。

- **内容与 `v1.1.0-beta.4` 完全一致**，仅版本号变更（`1.1.0-beta.4` → `1.1.1`）；无新增功能、无破坏性变更，可从早期版本平滑升级
- 大跨度的 Admin 运维面更新（探针遥测、AMAS 多块看板、词书中心、RBAC、反馈工单中心及一批安全/性能修复）当时正在 Beta 通道测试（见 v1.1.2-beta.1），测试通过后再转正

---

## [v1.1.2-beta.1] — 2026-05-31 · Pre-release · Admin 运维面大版本更新

> ⚠️ **这是一次大跨度 Beta 更新。** 相较 v1.1.0-beta.4，本次包含 **15 个数据库迁移、约 90 个新增接口、Admin 管理界面大面积重构与多块全新看板**。变更面非常广，**升级前请务必通读「升级前必读」并先备份数据库**。强烈建议先在非关键实例验证，确认无误后再用于生产。
>
> 适用对象：自托管本服务的运维者 / 管理员。普通学习端用户无需操作。

### ⚠️ 升级前必读（务必逐条确认）

1. **先手动备份数据库，再升级。** 升级后每日自动备份会**跳过首个周期**，刚重启的时间窗口内没有当天的恢复点。升级前请在 Admin 后台「版本更新 → 备份」手动触发一次，或直接 SSH 备份 `data/learning.db`。
2. **数据库迁移是单向、不可逆的。** 升级成功后库结构会从当前版本前滚到最新（本次新增 15 个迁移步骤）。**一旦前滚，无法在保留新数据的前提下干净回退到旧版本。** 回退仅允许换回旧程序二进制（旧代码忽略新增列/表，可正常运行）；**禁止**恢复旧数据库备份（会丢失升级后产生的全部学习数据），除非库已损坏。
3. **升级失败时的自动回滚只还原程序与前端，不还原数据库。** 内置升级器健康检查失败时自动换回旧程序/旧前端，但不动数据库；本批迁移纯增量，旧程序可继续在已前滚的库上运行。
4. **当前运行版本必须 ≥ v1.1.0-beta.3 才能一键升级。** 更早版本旧升级器存在已知自重启死锁，需手动替换二进制。已实测从 v1.1.0-beta.4 可经 Admin 一键升级、无需手动干预。
5. **「广播」功能行为收紧。** 受众「最低版本号」非法（如 `1.2` / `v2`）或受众过滤后无人匹配，现返回明确错误，不再静默返回「已发送（0 人）」。

### ✨ 新增功能

- **AMAS 引擎运维看板**：指标看板（核心 KPI / 算法分布 / 命中率·延迟·疲劳·奖励聚合）、版本对比（双配置并排比命中率·P95·ensemble·奖励·异常率·7 日留存，含 epsilon 等每版本参数）、决策直方图 / 疲劳时序 / MDM 热力图 / ELO 散点 / 阶段流转 / 学习风格聚类 / 异常 feed；灰度（Canary）支持按平台·账龄·活跃度人群过滤且逐用户真实生效
- **探针遥测数据采集看板**：遥测采样数据实时可视化采集面板
- **词书中心**：词书导入、筛选、预览、更新检查与同步管理
- **管理员角色（RBAC）**：super_admin / admin 分级，邀请·改角色·删管理员·签发 API Key 等特权仅超级管理员可执行
- **反馈工单中心**：升级为完整工单流（CSAT 评分、附件、设备画像、合并去重）
- **其他**：设置分区化、每日自动数据库备份、数据分析看板（留存矩阵 / 用户状态分布）、系统监控看板（健康 / 数据库 / 公网探针 / AMAS 事件）

### 🐛 安全与性能修复

- **安全**：修复管理员越权提权（特权接口此前仅校验「是否管理员」、不校验角色）→ 强制超级管理员鉴权；反馈导出 CSV 公式注入防护（`= + - @` 开头不再被 Excel 当公式执行）；反馈提交字段边界校验（CSAT 分值 / 附件数量·长度 / JSON 体积上限）
- **正确性**：灰度人群过滤真正生效（此前保存却被引擎忽略致全量下发）；灰度创建/扩量修复（此前稳态下可能零用户进入、监控拿不到样本致自动回滚失效）；广播静默零发送修复（见升级前必读 5）；指标时间戳按 UTC 跨时区归一；版本对比配置参数按版本快照解析（epsilon 不再错填为当前线上值）；反馈合并事务化
- **性能**：留存矩阵查询 676 → 1（双层逐格查询重写为单条聚合）；批量封禁/解封 800 → 1 次往返（单事务批量）

### ⚙️ 技术变更摘要

- 数据库迁移 m022–m036（共 15 步），全部为新增列（带默认值）/ 新增表 / 新增索引 / 种子数据，**无破坏性变更、无数据回填**，可重入；已用生产数据快照实测前滚成功（21 → 36，完整性校验通过）
- 后端约 90 个新增接口；Admin 前端约 190 个文件改动；后端测试 + 前端测试（组件 1010 + 端到端 52）均通过

---

## [v1.1.0-beta.4] — 2026-05-26 · Pre-release · 客户端/服务器时钟漂移三层防御 + install.sh 修坑

### 真根因复盘

在 boxd.sh 等 microVM 平台首次部署 wordforge 时碰到诡异现象：管理员登录成功后立刻被踢回登录页，浏览器 Network 显示后端 verify 返回 401。后端单元测试 verify 一切正常，本机自测也没问题 —— 跨端就崩。

最终定位到 **VM 时钟漂移 −10h48m**：

| 角色 | 用的时间 | 看到 token 的状态 |
|---|---|---|
| 后端签 `iat`/`exp` | 漂移后的本机时间（≈10h 前） | 未来 2h 内有效 |
| 浏览器 `isTokenExpired` | 用户系统的真实时间 | 已过期 8h+ |

浏览器视角 `Date.now() / 1000 >= exp` → `getAdminToken()` 主动 `storage.remove()` → 跳登录页。后端 verify 用同样漂移的时间，所以 in-VM curl 测试 200，但浏览器就是不接受。

microVM 上 `systemd-timesyncd` 因被 systemd 识别为 container 而拒启 (`ConditionVirtualization=!container was not met`)，`chrony` 与 `time-daemon` 虚拟包冲突装不上。靠手动 `ntpdate` 一次只能撑到下次 suspend/resume。**必须在代码层做防御**。

### 修复 1：客户端时间对齐（核心）

- **新增** `src/middleware/server_time.rs`：每个响应注入 `X-Server-Time: <unix_secs>` header
- **新增** `frontend/src/lib/clockSkew.ts`：fetch 拦截 `X-Server-Time`，计算 `skew = server - client` 持久化 localStorage（24h 过期），导出 `nowSecs()`
- **修改** `frontend/src/lib/token.ts` 的 `isTokenExpired` 改用 `nowSecs()` 替代 `Date.now() / 1000`
- **修改** `src/main.rs::build_cors_layer` CORS `expose_headers` 增加 `x-server-time`（跨域部署必需）

效果：无论服务器时钟漂多大（10h 也好），前端用 skew 修正后与服务器视角对齐，token 永不被误判过期。

### 修复 2：服务器启动 + 周期时钟自检

- **新增** `src/clock_health.rs`：并发探测 `cloudflare/google/apple` 三家的 HTTPS `Date` header（RFC 7231），取中位数对比本机时间
- **为什么不用 SNTP**：UDP/123 在 microVM / 出云防火墙 / 企业内网常被拦截；HTTPS/443 几乎必通；精度 ±2s 足够识别业务级漂移
- 漂移 > 60s（`DRIFT_DEGRADED_THRESHOLD_SECS`）→ `ERROR` 日志（含具体 `ntpdate` 修复命令）+ `/health` 状态 `degraded`
- 启动时 detached spawn 一次（不阻塞 `listen`）+ 每小时复查
- Warn-only：不 panic 启动流程，但让运维在被误诊为业务问题之前就看到根因

### 修复 3：`/health` 暴露时钟状态 + Admin Banner

- `/health` 响应增加 `services.clock`：`{ status, driftSecs, lastCheckAt, thresholdSecs }`
- 新增 `frontend/src/components/admin/ClockDriftWarning.tsx`，挂在 `AdminLayout` header 下方
- `clock.status === "drifted"` 时显示红色 banner，引导执行 `sudo ntpdate -u pool.ntp.org`

### 修复 4：`install.sh` 缺 `mkdir -p data` 导致首次启动死循环

`DATABASE_URL=./data/learning.db` 默认值，但 `install.sh` 没预建 `data/`。`wordforge` 启动时 `unable to open database file` → systemd `Restart=always` 立即重启 → 死循环。

修复：`install.sh` 在 `mkdir -p "$INSTALL_DIR"` 之后显式 `mkdir -p "$INSTALL_DIR/data"`。

### 修复 5：`install.sh` 生成的 `.env` `CORS_ORIGIN` 默认值会坑跨域部署

`.env.example` 的 `CORS_ORIGIN=http://localhost:5173` 是 dev 友好默认，但 `install.sh` 直接拷贝过去会让生产环境跨域部署直接坏。

修复：`install.sh` sed 写 `.env` 时在 `CORS_ORIGIN=...` 前注入两行提示注释，让用户首次部署就看到「部署时改为你的实际域名」的指引。

### upgrade ladder

| 当前生产 binary | 走 admin 一键升级到 |
|---|---|
| v1.1.0-beta.3 | **可以走 admin 一键升级** 到 beta.4（watcher 已 fork-exec 化）|
| v1.0.0 / v1.1.0-beta.{1,2} | **必须先 SSH 手动 swap 到 beta.3 再升 beta.4**（旧 binary 的 spawn_replacement 死锁修不了，参考 beta.3 release notes 修复 1）|

### 验证

- `cargo test --lib`：646 passed（含新增 `clock_health` 4 cases + `server_time` middleware 1 case）
- `vitest run tests/lib/clockSkew.test.ts`：10 passed
- `vitest run tests/lib/token.test.ts`：24 passed（向后兼容）
- `pnpm build`：全量构建通过

### 兼容性 / 破坏性

- **零新依赖**（`reqwest` / `chrono` / `futures` 已有；前端无新 npm 包）
- **零新 env 变量**
- `/health` schema 向后兼容（新增字段）
- `isTokenExpired` 语义在「未拿到 server time 时 skew=0」回退到原行为
- CORS `expose_headers` 增量增加 `x-server-time`

---

## [v1.1.0-beta.3] — 2026-05-23 · Pre-release · 自更新死锁根治 + backup stale 清理

### 真根因复盘

v1.0 引入 M0-R3 父进程 60s 健康监督机制时，**没同步改 `spawn_replacement` 的 sh wrapper "等 parent 退出后 exec" 逻辑**，形成死锁：

| 角色 | 逻辑 | 等的事 |
|---|---|---|
| sh wrapper | `while kill -0 $parent; do sleep 0.2; done; exec new_binary` | 等 parent 退出 |
| parent | `for i in 60s { probe /health on localhost; if 200 then exit(0) }` | 等子进程 /health 200 |

新 binary 永远没被 exec → parent 探针打到自己（旧进程仍 listen）→ swap 阶段已开 maintenance flag → /health 返回 503 → 60s 全失败 → parent 走 rollback `return Err`，但**不 exit** → sh wrapper 孤儿永久等。systemctl status 里那个 24h+ 的 sh wrapper PID 就是该死锁的物理证据。

**v1.0 引入 M0-R3 后 admin 一键升级从未在生产真正成功过**。所有"成功"的升级（v1.0-rc.1/rc.2/v1.0 GA → v1.0）都是这之前 v0.x 时代的 sh wrapper + parent exit(0) 风格（无监督），M0-R3 之后所有升级理论上都会 60s 卡死回滚。

事故链（2026-05-23）：v1.0 → v1.1.0-beta.1 升级 60s timeout → v1.1.0-beta.1 加了 stderr dup2 救场 → v1.0 → beta.2 又踩 backup db / static dir stale 残留陷阱反复失败 → SSH 手动 swap beta.2 + 写本 release 真正解死锁。

### 修复 1：fork watcher + parent 立即 exit（Hybrid 设计）

`src/services/updater.rs`：

- **彻底删** `apply_locked` 内的 M0-R3 60s health check loop + spawn_replacement sh wrapper 调用
- **新增** `WatcherArgs` / `spawn_watcher_then_exit_parent` / `run_watcher` / `watcher_probe_health` / `watcher_rollback` / `watcher_update_audit_outcome`
- 新流程：parent swap 完成 → `libc::fork` watcher 子进程（detached / setsid / stdio→/dev/null）→ parent `std::process::exit(0)` → systemd `Restart=always` 在 `RestartSec=5` 后启动新 binary
- watcher 独立 sleep 10s 给 systemd + binary startup → 60s loop `curl /health`：
    - 通过 → rusqlite 直接 `UPDATE update_audit_log SET outcome='success'` → watcher exit
    - 60s 超时 → `rename bin/static` 回滚 + 标 `.failed` 保留 forensics + `kill SIGTERM` 当前 wordforge 主进程 → systemd Restart 起 rolled-back v1.0 + UPDATE outcome=`rolled_back` → watcher exit
- watcher 用 `curl` 而非 `reqwest`：fork 后 tokio runtime 状态不安全，sync HTTP 用 shell `curl` 最稳

权衡：
- ✅ 解死锁：parent 不再等子进程 health 200，立即 exit 让 systemd 接管
- ✅ 保留 60s 自动回滚能力（watcher 独立监督）
- ✅ admin UI 看 audit log 能拿到终态（success / rolled_back / applied_pending_watcher）
- ❌ watcher 用 `pgrep` + `SIGTERM` kill 主进程要求 wordforge 用户能 kill 同 user 进程（systemd User=wordforge 跑的进程，OK）
- ❌ 失败时仍依赖 systemd Restart=always 重启 rolled-back binary，不是 100% 即时

### 修复 2：rename-to-backup stale 清理

事故里反复踩的 `learning-v1.0.0.backup.db` + `static.v1.0.0` 残留陷阱：

- **`src/store/mod.rs::backup_to`**：`VACUUM INTO` 之前先 `remove_file(dst)`，避免 SQLite vacuum.c 主动检测 + `SQLITE_ERROR: output file already exists`
- **`src/services/updater.rs::apply_locked` swap 段**：`rename(bin_path, bin_backup)` 和 `rename(static_path, static_backup)` 之前都先 `remove_file` / `remove_dir_all` 目标
- watcher rollback 路径同样：失败 binary/static 标 `.failed` 之前先 remove 同名残留
- `StoreError` 加 `Io(#[from] std::io::Error)` 变体（之前 `Sqlite`/`Pool`/...缺 IO）

### 修复 3：audit log 中间态 outcome

handler 插入 `outcome='in_progress'` → updater 在 fork watcher 前更新为 `outcome='applied_pending_watcher'`（明确 swap 已成功、watcher 接管中）→ watcher 最终更新 `success` / `rolled_back`。admin UI 能区分"升级中"与"watcher 接管期间"两种状态。

### ApplyContext 变更

```rust
pub struct ApplyContext {
    pub channel: Channel,
    pub target_tag: String,
    pub health_url: String,
    pub on_rollback: Box<dyn Fn(String) + Send + 'static>,
    pub on_maintenance: Box<dyn Fn(bool) + Send + 'static>,
    pub task_id: String,  // v1.1.0-beta.3 新增：传给 watcher 用于 UPDATE audit_log
}
```

`on_rollback` callback 在新设计里**不再被 apply_locked 调用**（watcher 独立进程无法回调到 main process closure）；但 `ApplyContext` 字段保留兼容 handler 不需要改造。

### upgrade ladder

| 当前生产 binary | 走 admin 一键升级到 |
|---|---|
| v1.0.0 / v1.1.0-beta.{1,2} | **必须 SSH 手动 swap 到 beta.3**（旧 binary 的 spawn_replacement 死锁修不了） |
| v1.1.0-beta.3 | **可以走 admin 一键升级**到后续版本（watcher 设计取代死锁的 sh wrapper） |

### 版本号

`Cargo.toml` + `Cargo.lock`: `1.1.0-beta.2` → `1.1.0-beta.3`

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

- 🚀 **领域事件总线基础设施**（P1-S2，commit 568f294）：records 写库成功后旁路 emit
  `DomainEvent::RecordCreated`，事件总线（`src/services/event_bus.rs`）内存单向广播 +
  per-receiver 缓冲。**此 RC 仅落基础设施 + 计数旁路 consumer，AMAS 仍走 handler 内同步通路
  （非异步消费）**；outbox 持久化 + AMAS 真异步消费（opt-in）见 v1.1.3 S2-1。新增
  `tests/event_bus.rs` 单测覆盖 emit/recv 与背压 drop 行为。

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
