# 维度四：发布 / 运维 / 文档契约盘点（release-keeper）

> 调研时间：2026-05-21
> 当前版本：v0.6.0-beta.4（`Cargo.toml:3`）；最近 stable：v0.5.6；最近 beta：v0.6.0-beta.3
> 目标：把 GA 后的发布流、运维 Runbook、对外契约盘清楚，输出 v1 之前必须收口的清单。

---

## 1. 发布流现状

### 1.1 CI / CD workflows 全景

| 文件 | 触发 | 作用 | 健康度 |
|---|---|---|---|
| `.github/workflows/release.yml` | `push tags v*` | 双架构（x86_64 / aarch64）musl 交叉编译 → tarball + sha256 → GH Release，**prerelease 规则已硬编码** | 健康（v0.6.0-beta.3 起加 `prerelease: ${{ contains(github.ref_name, '-') }}`，`release.yml:105`） |
| `.github/workflows/coverage.yml` | push/PR `main`/`develop` | 后端 + 前端覆盖率，门槛 lines/functions/regions ≥ 80%；忽略 `main.rs`/`bin/`/`logging.rs`/`lib.rs`/`llm_advisor.rs`/`heartbeat_watchdog.rs`/`update_checker.rs`/`updater.rs`（`coverage.yml:23`） | 健康；但 5 个不可测入口的忽略名单需要每加一个 worker / service 同步维护 |
| `.github/workflows/deploy-docs.yml` | push `main` 且 `docs/**` 变更 | VitePress 构建并部署到 GitHub Pages | 健康 |
| `.github/workflows/e2e-tests.yml` | push/PR `main`/`develop` | Playwright E2E（chromium）；分支 `verify-auto-update-v044` 单独跑 `scripts/verify-release-auto-update.sh` | 健康，但 v044 verify 脚本的契约**已过期**（见 §1.5） |
| ~~`.github/workflows/verify-auto-update-v043.yml`~~ | ~~仅 `workflow_dispatch`~~ | ~~v0.4.2→v0.4.3 升级冒烟~~ | **已删除**（commit `a022e6b`，2026-06-06）：连同 `scripts/verify-auto-update-v043.sh` 一并移除，由 `e2e-tests.yml` 的 `verify-auto-update-v044` job（走 `scripts/verify-release-auto-update.sh`）取代 |

### 1.2 stable / beta 双通道

- 后端：`src/services/updater.rs:166-180`（`UpdaterCache.stable / .beta`）+ `parse_release_list_payload` 从单次 `/releases?per_page=N` 分流 `stable_latest = max semver where prerelease=false` 与 `beta_latest = max semver overall`。
- API：`/api/admin/updates/{status,check,apply}`（`src/routes/admin/updates.rs:25-30`），`apply` 必须传 `channel: "stable" | "beta"`（`updates.rs:97-102`）。
- 前端：`UpdatesPage.tsx` 主卡 = 稳定通道；Beta 通道收在 `<Collapsible>`，有更新时亮 badge（`UpdatesPage.tsx:246-256`）。
- 客户端契约：`AdminUpdateStatus` 包含 `stable / beta: ChannelStatus | null`、`applyTask?`（`frontend/src/types/admin.ts:89-124`）。

### 1.3 prerelease 规则锁死

`release.yml:105` 已显式 `prerelease: ${{ contains(github.ref_name, '-') }}`，断绝 v0.6.0-beta.1/.2 时 softprops 自动判定行为不一致的根因。规则：

- tag 含 `-`（v0.6.0-beta.N / v1.0.0-rc.1）→ Pre-release
- 不含 `-`（v1.0.0 / v0.5.6）→ Latest stable

**校验缺口**：CI 没有阻止"打 stable tag 时漏 `-`"或"打 beta tag 时少 `-rc.`"的人为错误；建议补一个 `pre-release-tag-lint` step。

### 1.4 release notes 渲染

`release.yml:100` 用 `softprops/action-gh-release@v2` 的 `generate_release_notes: true`，由 GitHub 自动生成。`#58` 加了 admin 后台 `ReleaseNotesMarkdown.tsx`（自实现 markdown 子集，零 XSS，覆盖 ## H2/### H3/列表/code fence/inline `**bold** / \`code\` / 链接`）。**已知约束**：表格、HTML inline、图片不渲染，按 `<p>` 原样显示——对 release notes 场景够用，但未来若 release notes 引入截图/表格会回退到纯文本。

### 1.5 已过期 / 不一致的发布脚本与契约

| 项 | 现状 | 不一致点 |
|---|---|---|
| `scripts/verify-release-auto-update.sh:118-127` | jq 校验 `.data.latestVersion == $latest`、构造 `{"targetVersion":..., "confirmCurrentVersion":...}` | v0.6.0-beta.3 起契约改为 `{stable, beta}` 嵌套 + apply 必带 `channel`；这两个脚本只覆盖单通道老路径，**升级到 v044 的 verify 当下能跑只是因为后端单 release 时 fallback 让 `stable=beta=same`**（`updater.rs:835-842`），新发布双通道并存时立即破。 |
| ~~`scripts/verify-auto-update-v043.sh`~~ | **已删除**（commit `a022e6b`） | 原因：仍用单通道 flat 字段 + `cargo build --release` 现编后端（违反"禁服务器编译"规则）。已随工作流一并移除 |
| `docs/auto-update.md:30-46, 158-160` | 流程图与 API 表只画 `apply` 单通道路径；无 `channel` 参数说明、无 stable/beta 概念 | 与 v0.6.0-beta.3 落地代码不一致 |
| `docs/openapi.yaml:4` | `version: 0.4.3` | 距当前 v0.6.0-beta.4 落后 4 个 release；规格也极简（10 个端点 stub），与 `docs/api-endpoints.md` 的 100+ 端点详表不同源 |
| `docs/api-endpoints.md:2353-2371` SSE 事件表 | 列了 6 个（`amas_state / maintenance / update_available / telemetry_request / banned / unbanned / data_corrupted`） | 后端 `SseEvent` 枚举（`src/state.rs:25-84`）实际 9 个变体——缺 `new_llm_suggestion / release_available / update_progress / probe_request / probe_confirm`，且 `update_available` 文档示例的 payload 还是 v0.3.x 格式 |
| `src/routes/admin/monitoring.rs:102-178` `/admin/monitoring/check-update` | v0.4.x 遗留，前端 `AdminDashboard.tsx / AdminWordbookCenterPage.tsx` 还在用，prerelease 排序仍走字符串比较（`is_newer`），不识别 `-beta` semver pre-release 排序 | 与新双通道 `/admin/updates/status` 并存；语义重复且老接口不识别 v0.6.0-beta.X 排序，会在 dashboard 上误报 `hasUpdate: false` |

---

## 2. admin/updates 当前能力 vs GA 后需补能力

### 2.1 当前已具备（v0.6.0-beta.3 / 0.6.0-beta.4）

| 能力 | 实现位置 | 状态 |
|---|---|---|
| 双通道（stable / beta） | `updater.rs:165-181, 820-880` + `updates.rs:75-93` | ✅ |
| 异步 apply（202 + taskId + 轮询 `applyTask`） | `updates.rs:104-207` + `state.rs:97-112` | ✅（v0.5.2 之后的设计） |
| SSE 推 `release_available` / `update_progress` | `state.rs:49-57`、`updates.rs:88-92, 162-170` | ✅ |
| sha256 校验 + zip-slip + symlink 拒绝 + 大小限制 | `updater.rs:496-501, 895-931` | ✅ |
| DB 自动备份（VACUUM INTO）+ 旧版本保留 N=2 | `updater.rs:513-523, 682-724` | ✅ |
| fork-exec 自重启（脱离 systemd 依赖） | `updater.rs:582-588, 979-1009` | ✅ |
| GitHub release 下载镜像（gh-proxy / ghproxy.net） | `updater.rs:592-598`、`.env.example:71-76` | ✅（v0.5.4） |
| reqwest 双 client（30s API total / 60s per-chunk read） | `updater.rs:208-220` | ✅（v0.5.4） |
| ETag 条件请求节省 GitHub API 额度 | `updater.rs:294-372` | ✅（带 token 时不计 primary rate limit） |
| strict-mode 豁免 `/admin/*` + SSE + `/status` | `middleware/strict_mode.rs:40-47` | ✅（v0.5.2） |
| install.sh 一键安装（systemd + 系统用户 + 随机密钥） | `install.sh` 119 行 | ✅ |
| confirmCurrentVersion 防误操作、并发 lock、downgrade 拒绝 | `updates.rs:111-118, 119-128`、`updater.rs:416-450` | ✅ |
| 二次确认弹窗 + 进度条 + 终态保留显示 | `UpdatesPage.tsx:296-336, 258-279` | ✅ |

### 2.2 GA 之前必须补齐的能力（按优先级）

| 优先级 | 能力 | 现状 | 建议 |
|---|---|---|---|
| **P0** | 多实例 / 集群升级 | 完全不支持（fork-exec 默认单实例；并发文件锁仅本机） | 暂留为"单机部署"前提；如要上多实例，需要把 leader 选举 + 串行升级写进 runbook |
| **P0** | 升级失败自动回滚（不止 rename 中途）| 仅 staging→swap 阶段失败有 rollback；fork-exec 后子进程起不来不会回滚 | 引入"启动后 N 秒自检"：新进程必须在 60s 内 `GET /health` 返回 200，否则回滚到 `wordforge.{old_tag}` |
| **P0** | systemd 单元 `Restart=on-failure` 对 `exit 0` 不重启 | 内存里第 4 坑点：`install.sh:99` 是 `Restart=on-failure`；fork-exec 的 `exit(0)` 父进程不会被 systemd 拉起，依赖子进程拿到端口 | 文档化"用 install.sh 起不能用 `systemctl restart`，必须靠 fork-exec"；或改 `Restart=always` 让 systemd 在子进程也挂时兜底（要测会不会与 fork-exec 子进程冲突）|
| **P1** | 灰度 / 金丝雀（按用户百分比 / 客户端版本切流） | 完全不支持 | 后端 strict-mode 已具备"按版本拒绝"能力（`min_client_version`），可基于此扩展"≥ 某版本走新路径"的功能开关；但灰度发布本身需要先有"多实例 + 服务发现" |
| **P1** | 升级前 dry-run（仅下载 + sha256 + 解压到 staging，不替换 swap） | 没有 | 加 `apply` 一个 `dry_run: true` 模式：跑完 step 1-3 就停，把 staging 路径返回 admin |
| **P1** | 升级中后端流量保护（maintenance 模式自动开 / 关） | `maintenance.rs` 中间件存在但 apply 流程**不会自动切**到维护模式 | apply 进入 `swapping` 时自动打开 maintenance，restarting 完成或失败时关闭（要小心 fork-exec 后父进程没有机会 reset） |
| **P1** | 灾难恢复（DB 备份外迁） | 仅备份在同一磁盘的 `data/learning-{tag}.backup.db`，机器丢就全丢 | 加 `BACKUP_S3_BUCKET` / `BACKUP_RSYNC_TARGET` 可选 env，apply 成功后异步推走 |
| **P2** | 升级历史审计（谁在什么时间升了什么版本） | tracing::warn 记一行，写不进表 | 加 `update_audit_log` 表：admin_id, from_version, to_version, channel, started_at, completed_at, outcome, error |
| **P2** | release notes 表格 / 图片支持 | `ReleaseNotesMarkdown.tsx` 是手写 parser 子集，不支持表格 / HTML / 图片 | 引入 marked + dompurify（约 +20KB gzip）或保持现状只在 release notes 里禁用复杂语法 |
| **P2** | minisign / cosign 二进制签名 | 仅 sha256 校验；`docs/auto-update.md:140` 已自陈"GitHub 账号被入侵会让验证失效" | release.yml 加 minisign 签名 step，updater 在 `fetch_sha256` 后追加签名校验；公钥嵌入二进制 |

---

## 3. 客户端 × 后端契约：稳定性级别建议

### 3.1 当前对齐状态

| 维度 | 状态 | 证据 |
|---|---|---|
| iOS REST 契约 | 0 P0 / 0 P1 | `docs/alignment.md:144`（第三轮审计） |
| 遥测通道 schema | 0 P0 / 0 P1（strict-mode middleware + payload 校验落地） | `docs/alignment.md:145` |
| Admin 控制台契约 | 0 P0 / 0 P1（metrics camelCase + monitoring 包装 + feedback page） | `docs/alignment.md:146` |
| AMAS schema 双向 | 0 P0 / 0 P1（default_w 对齐 + codegen 全量类型 + 4 字段兜底） | `docs/alignment.md:147` |

但**报告本身已 2 天部分过期**：
- 报告说"frontend.package.json 加了 `gen:amas-types`"——✅ 仍在 `frontend/package.json:13`
- 报告说 `docs/alignment.md` 写于 2026-05-19；这之后又落了 v0.6.0-beta.{1,2,3,4}（双通道、ErrorBoundary 修、release notes md 渲染、WordState lowercase 重命名、favorites paginated 修）——这些**没有反映在 alignment.md**，需要在 v1 之前跑第四轮 cross-validator。
- `frontend/src/types/wordState.ts` 与新落地的 `wire 序列化 lowercase`（`d0325f8` "fix(word-states)!"）的同步状态没在 alignment 表里——这是个 breaking change，commit 标了 `!`。

### 3.2 v1 之后的稳定性级别建议（按 API/事件流）

> 标记法：`v1 stable` = 禁破坏性变更，需 deprecation policy；`v1beta` = 可破坏，需 release notes 突出；`v0/internal` = 随意。

#### 3.2.1 用户域（学习端 iOS / Web 学习端）

| 端点 / 事件 | 建议级别 | 理由 |
|---|---|---|
| `POST /api/auth/{login,register,refresh,forgot-password,reset-password,verify-reset-token}` | **v1 stable** | iOS 装机即用，break 需所有客户端强升 |
| `POST /api/auth/logout` | **v1 stable** | 同上 |
| `GET/PUT /api/user-profile` | **v1 stable** | iOS 设置页直依赖 |
| `GET /api/words` / `GET /api/words/:id` | **v1 stable** | 学习核心循环 |
| `POST /api/records` | **v1 stable** | 学习核心循环；目前 `self_rating: Option<u8>` 已对齐（alignment v3） |
| `GET /api/records` | **v1 stable** | 历史页 |
| `POST /api/learning/session*` | **v1 stable** | 学习会话核心 |
| `GET/PUT /api/word-states/:wordId` | **v1 stable**（但 `d0325f8` 的 wire lowercase 已是 breaking） | 客户端必须升到新 wire 才能读；建议把这次记为 v1 stable 的"基准格式" |
| `GET/POST/DELETE /api/word-favorites` | **v1 stable** | iOS 收藏页直依赖；`fb93944` paginated 修复使 `data.data` 契约稳固 |
| `GET/POST/PUT/DELETE /api/word-notes` | **v1 stable** | 同上 |
| `GET /api/wordbooks/*`、`/api/wordbook-center/*` | **v1 stable** | iOS 词书页 |
| `GET/PUT /api/study-config` | **v1 stable** | iOS 设置 |
| `GET /api/analytics/*` | **v1 stable** | iOS 统计页 |
| `GET /api/realtime/events` (SSE) | **v1 stable** | 事件类型可加，**event name 字符串不可改**；payload 增字段 OK，去字段 break |
| `POST /api/telemetry` | **v1beta** | strict-mode payload 校验仍可能加新必填字段 |
| `POST /api/feedback` | **v1 stable** | 用户反馈通道 |
| `POST /api/notifications/*` | **v1 stable** | iOS 推送配置 |
| `POST /api/content/*` | **v1beta** | 内容增强（释义/词根/例句）尚在打磨，结构可能扩展 |
| `GET /api/status` | **v1 stable** | 客户端版本探测 |
| `GET /health` | **v1 stable** | LB / k8s 探针 |
| `/api/v1/*` 兼容层 | **v0 / 永久冻结** | `routes/v1.rs:1-11` 已带"刻意绕过 AMAS"警告；策略：永久冻结现有 4 个端点的字段集，不接受任何破坏性变更也不增功能，未来新客户端禁用 |
| SSE event `amas_state / maintenance / banned / unbanned / data_corrupted` | **v1 stable** | iOS 已订阅 |
| SSE event `update_available / telemetry_request` | **v1 stable** | iOS 已订阅 |
| SSE event `data_corrupted` | **v1 stable** | 数据自检通道 |

#### 3.2.2 管理员域（Web admin 后台）

| 端点 / 事件 | 建议级别 | 理由 |
|---|---|---|
| `/api/admin/auth/*` | **v1 stable** | 管理员登录闭环 |
| `/api/admin/users*` | **v1 stable** | 用户管理 |
| `/api/admin/stats` | **v1beta** | KPI 计算口径仍在迭代 |
| `/api/admin/analytics/*` | **v1beta** | KPI 计算口径仍在迭代 |
| `/api/admin/monitoring/{health,database}` | **v1 stable** | 运维 dashboard |
| `/api/admin/monitoring/check-update` | **v0 / 弃用待删** | 已被 `/admin/updates/*` 替代；公开弃用 → v1 GA 时删 |
| `/api/admin/updates/{status,check,apply}` | **v1 stable** | 自更新核心；`channel` enum 仅 stable/beta，扩枚举值是破坏性 |
| `/api/admin/broadcast*` | **v1 stable** | 维护通知通道 |
| `/api/admin/settings/*` | **v1 stable** | 设置存取 |
| `/api/admin/feedback/*` | **v1 stable** | v0.6.0-beta.2 修复后稳定 |
| `/api/admin/amas/config*` | **v1 stable**（但 enum 值可加） | schemars codegen 保证向后兼容；新增字段必须 `default = ...` |
| `/api/admin/amas/config/schema` | **v1 stable** | 前端 codegen 依赖 |
| `/api/admin/amas/{metrics,monitoring,version*}` | **v1beta** | AMAS 子模块仍可能调整指标维度 |
| `/api/admin/amas/advisor/*` | **v1beta** | LLM 顾问产物结构会扩展 |
| `/api/admin/clients*`、`/api/admin/telemetry*` | **v1beta** | strict-mode 字段集会增 |
| `/api/admin/probe*` | **v0 / internal** | 远程探针是运维诊断工具，不对外承诺；schema 可随意改 |
| `/api/probe/results` | **v0 / internal** | 同上 |
| SSE event `new_llm_suggestion` | **v1beta** | LLM 顾问通道 |
| SSE event `release_available / update_progress` | **v1 stable** | admin 后台升级 UI 直依赖 |
| SSE event `probe_request / probe_confirm` | **v0 / internal** | 远程探针 |

### 3.3 Deprecation policy 建议

1. **公告窗口**：v1 stable 端点弃用 = 公告 ≥ 2 个 minor 版本（≥ 6 个月，按当前节奏）；v1beta = ≥ 1 个 minor。
2. **运行时信号**：弃用端点 response header 加 `Deprecation: <date>` + `Sunset: <date>`（[RFC 8594](https://datatracker.ietf.org/doc/html/rfc8594)），客户端 logger 应抓这两个 header 并告警。
3. **代码标记**：handler 加 `#[deprecated(since = "v1.M.0", note = "use /new-path")]`；客户端 OpenAPI / schema codegen 把 deprecated 端点标灰但不删（前端 lint 拦截新引用）。
4. **删除时机**：仅在 next major（v2）切换时移除；v1 全生命周期保留旧端点（即使返回 410 Gone 也比删了路由强，留个明确的错误码）。
5. **breaking 序列化变更**（如本次 `WordState wire lowercase` `d0325f8`）：禁止在 v1 内引入；只在 major bump 时同步打掉客户端最低版本门（strict-mode `min_client_version`）。

---

## 4. 文档完整度矩阵

> ✅ 已有；⚠️ 过期；❌ 缺失

### 4.1 用户文档（学习端 / iOS 用户）

| 主题 | 现状 | 说明 |
|---|---|---|
| 项目简介 | ✅ `docs/guide/introduction.md` | 完整 |
| 快速开始（开发者） | ✅ `docs/guide/getting-started.md` | 偏开发者本地跑起；缺最终用户视角（"我下载客户端怎么用"）|
| 客户端下载 / 安装 | ❌ 缺失 | iOS / Android / Web 客户端入口、版本对应矩阵、最低 OS 要求 |
| 学习功能使用手册 | ❌ 缺失 | 卡片操作、收藏、笔记、SRS 触发逻辑等用户视角说明 |
| 账户与隐私 | ❌ 缺失 | 注册 / 删除账号 / 数据导出 / GDPR 合规说明（关乎合规上架）|
| 常见问题 / FAQ | ❌ 缺失 | 安装、闪退、同步失败的自助排查 |

### 4.2 运维文档

| 主题 | 现状 | 说明 |
|---|---|---|
| 一键安装 install.sh | ✅ `install.sh` 119 行 | 系统用户、systemd、随机密钥、`.env` 保留更新 |
| 自更新数据流 | ⚠️ `docs/auto-update.md` 170 行 | **缺双通道（stable/beta）说明；`apply` 请求体缺 `channel` 字段说明**——v0.6.0-beta.3 后没同步更新 |
| 手动回滚 | ✅ `docs/auto-update.md:102-129` | 完整：杀进程 / 改名 / DB 恢复 / 重启 |
| 环境变量参考 | ✅ `.env.example`（含 self-update + AMAS + LLM + 镜像前缀） | 完整 |
| systemd 单元 | ✅ `install.sh:86-110`（嵌在脚本里）| **没单独文档化**；运维改 Unit 文件不知道有哪些环境注入项 |
| nginx / 反代配置 | ❌ 缺失 | 内存里说生产用了 nginx，但仓库无任何 nginx 参考配置 |
| 备份 / 灾恢 runbook | ⚠️ 部分 | DB 备份策略只在 `auto-update.md` 自更新章节顺带提；缺独立 backup runbook |
| 监控与告警 | ❌ 缺失 | `/admin/monitoring/{health,database}` 是 admin UI；没文档化"如何对接外部 prometheus / 怎么读 metrics_flush 写到哪张表 / 如何配 alert" |
| 日志策略 | ❌ 缺失 | `LOG_DIR` / `ENABLE_FILE_LOGS` env 存在，但无文档说明日志轮转 / 等级 / 上传 |
| Prometheus / OpenMetrics 暴露 | ❌ **未实现** | `src/workers/metrics_flush.rs` 只写库；没 `/metrics` 端点；运维侧无可被外部抓取的指标 |
| 多实例 / 高可用部署 | ❌ 缺失 | 当前隐式单机；无 leader 选举说明（`ENABLE_*_WORKER + WORKER_LEADER=true` 暗示有但无 runbook） |

### 4.3 开发者文档

| 主题 | 现状 | 说明 |
|---|---|---|
| 架构概览 | ✅ `docs/guide/architecture.md` | 整体骨架 |
| AMAS 入门 | ✅ `docs/guide/amas-intro.md` | 完整 |
| AMAS 调参管理后台 | ✅ `docs/amas-admin-console.md` | 完整 |
| AMAS schema codegen 工作流 | ✅ `docs/amas-schema-codegen.md` 94 行 | schemars + json-schema-to-typescript 双向链路 |
| AMAS 调参记录（2026-05-15） | ✅ `docs/amas-tuning-2026-05-15/*` 三篇 | 完整 |
| API 对接规范 | ✅ `docs/api-spec.md` 278 行 | 全局约定（认证、错误码、限流） |
| API 端点详表 | ✅ `docs/api-endpoints.md` 2850 行 / 112 节 | **SSE 事件表过期**（缺 4 个事件，1 个 payload 错版本）；监控 dashboard 调用的遗留接口未标"已弃用" |
| 客户端上传数据规范 | ✅ `docs/client-upload-data.md` 1017 行 | 完整 |
| UI 静态审计 | ✅ `docs/ui-audit.md` 493 行 | 完整 |
| 客户端 × 后端契约对齐 | ⚠️ `docs/alignment.md` 160 行 | 第三轮（2026-05-19），但之后 v0.6.0-beta.{1-4} 落地未跑第四轮 |
| OpenAPI 规格 | ⚠️ `docs/openapi.yaml` 94 行 / `version: 0.4.3` | **极简 stub + 版本号落后 4 个 release**；与 `api-endpoints.md` 不同源；没接 CI 生成 SDK |
| README.md（项目根） | ❌ **缺失** | 顶级 README 不存在；GitHub 仓库首页空白 |
| CONTRIBUTING.md | ❌ 缺失 | 没有贡献者指南、Conventional Commits 约束、PR 模板 |
| CHANGELOG.md | ❌ 缺失 | release notes 全靠 GH release_notes 自动生成，没归档到仓库 |
| SECURITY.md | ❌ 缺失 | 没有安全披露通道说明 |
| LICENSE 显式 | 未检查 | 仓库 Cargo.toml license 字段 / LICENSE 文件需要核 |

### 4.4 API 参考 / SDK 文档

- **OpenAPI** ⚠️：现有 `docs/openapi.yaml` 是手写残桩（94 行覆盖 10 个端点），版本号 0.4.3。**与代码不同源、与 `api-endpoints.md` 不同步、不在 CI 生成、不导出 SDK**。
- **schemars JSON Schema** ✅：仅覆盖 AMAS config 子结构（21 个）；REST DTO（Request/Response 类型）**未走 schemars**。
- **TypeScript 客户端 SDK** ❌：前端 admin 后台手写 `frontend/src/api/admin.ts`；没生成 SDK；iOS / Android 客户端各自手抄 endpoint。
- **OpenAPI codegen** ❌：没有 `npm run gen:openapi-types` 之类的链路。

### 4.5 故障排查

- ⚠️ `docs/auto-update.md:102-129` 有自更新回滚 runbook（手动 mv + DB 恢复），是仅有的故障 runbook。
- ❌ 缺：DB 写失败 / SSE 连接打满 / GitHub rate-limited / 磁盘满 / WAL 不收 / 重启循环 等场景的诊断步骤。
- ❌ 缺：`journalctl -u wordforge -f` 之外的"我应该看什么"对照表（错误码 → 应当看的日志关键字 → 修复手段）。

### 4.6 文档站导航完整度

`docs/.vitepress/config.mts` 侧边栏**未收录**以下文档（运维侧只能直链访问）：

- `auto-update.md`（自更新核心运维文档）
- `alignment.md`（客户端契约对齐审计）
- `amas-schema-codegen.md`（schemars codegen 工作流）
- `openapi.yaml`（OpenAPI 规格）
- `ui-audit.md`（UI 静态审计）
- `amas-admin-console.md`（虽在 nav 但未在 sidebar）

这意味着 GitHub Pages 上访客只能看到"入门 + API 文档 + AMAS"三大类，运维文档需要靠搜索框找到——v1 之前必须补 `运维` / `开发者参考` 两类 sidebar。

---

## 5. 运维 Runbook 清单

| Runbook | 状态 | 来源 / 缺口 |
|---|---|---|
| 首次安装部署 | **已有** | `install.sh` + `getting-started.md` |
| 升级（一键 admin） | **已有** | `auto-update.md` 数据流图 + `UpdatesPage.tsx` 二次确认 |
| 升级（手动） | **缺失** | 没文档化"SSH 上去手动替换二进制"——CI artifact + scp 的步骤 |
| 回滚（升级后崩） | **已有** | `auto-update.md:102-129` |
| 回滚（升级中崩） | **已有（自动）** | `updater.rs:957-977` rollback() |
| 回滚（升级成功后发现业务异常） | **过期** | 文档只覆盖"crash"场景；"功能行为变差"如何评估回滚需要人工判断，无 SOP |
| DB 备份外迁 | **缺失** | 只有本地 backup（自更新顺带）；无定期 rsync / S3 上传 |
| DB 灾难恢复（机器丢） | **缺失** | 同上 |
| systemd 单元维护 | **过期** | install.sh 嵌入了 unit；没单独文档化"改 unit 要 daemon-reload" |
| nginx / 反代 | **缺失** | 内存说生产挂 nginx，仓库无 sample.conf |
| TLS 证书续期 | **缺失** | 同上 |
| 流量保护（维护模式） | **半缺失** | `maintenance.rs` 中间件存在但**没在 admin UI 暴露开关**；只能通过环境变量重启切换（待补 `/admin/settings` 路径） |
| GitHub rate-limited 应急 | **半已有** | `auto-update.md:131-136` 说"超限返回 503"；缺"怎么填 token / 切镜像"的实操步骤索引 |
| SSE 连接打满 | **缺失** | `health.rs` 有 `sse_probe_ok` 但无"打满后怎么扩 + 怎么排查"runbook |
| 日志收集 / 轮转 | **缺失** | `LOG_DIR` env 存在；轮转策略未文档化 |
| 监控告警接入 | **缺失** | 无 prometheus 端点；admin UI 的 `/admin/monitoring/*` 只能人工看 |
| 性能基线 / 容量规划 | **缺失** | 见 `04-perf-warden.md`（由 perf-warden 产）|
| 密钥轮转（JWT/Admin/Refresh） | **缺失** | 没文档化"换密钥后所有 token 失效"的运维窗口 SOP |
| 用户删除 / GDPR 合规 | **半已有** | `delete_user` 后端已级联 `wb_center_imports`（alignment v3）；admin UI 是否有删除入口、合规 timeline 未文档化 |
| 灰度发布 | **缺失** | 见 §2.2 |

---

## 6. v1 之前必须收口的清单

按优先级排序；P0 = v1 GA 前必须；P1 = GA 后 30 天内；P2 = GA 后 90 天内。

### 6.1 契约 / 序列化（P0）

| # | 项 | 工时估 | 验收 |
|---|---|---|---|
| C1 | 跑第四轮 cross-validator 审计，把 v0.6.0-beta.{1-4} 期间的变更（双通道、ErrorBoundary、release notes md、word_states wire lowercase、favorites paginated）补进 `alignment.md` | 1 天 | `cargo test` 全过 + 新版 alignment.md merged |
| C2 | 把 `docs/openapi.yaml` 改为 schemars / utoipa 自动导出（与 amas codegen 同类），删手写 stub | 2 天 | CI 加 `cargo test --test openapi_schema_export` + `git diff --exit-code` |
| C3 | 把 SSE 事件表写进 OpenAPI（AsyncAPI 风格 description）或单独 `events.md`；补缺的 4 个事件 | 0.5 天 | `docs/api-endpoints.md:2353-2371` 更新 + 加 `release_available / update_progress / new_llm_suggestion / probe_request / probe_confirm` |
| C4 | 给所有 v1 stable handler 加 `#[deprecated(since = ...)]` 标记位预留；引入 `Deprecation` / `Sunset` response header 中间件 | 0.5 天 | 一个 `tests/deprecation_header.rs` 集成测试 |
| C5 | 明确 `/api/v1/*` 兼容层"永久冻结 + 不接新调用方"的弃用公告（`routes/v1.rs:1-11` 已警告，但没对外公告） | 0.2 天 | `api-endpoints.md` 第 18 节加红色 banner |
| C6 | 修 `verify-release-auto-update.sh` 的契约：用新 `{stable, beta}` 双通道 + `channel` 字段（`verify-auto-update-v043.sh` 已于 commit `a022e6b` 删除，无需再修） | 0.5 天 | 跑通 v0.6.0-beta.X → v0.6.0-beta.Y 升级冒烟 |

### 6.2 发布流（P0）

| # | 项 | 工时估 | 验收 |
|---|---|---|---|
| R1 | release.yml 加 `pre-release-tag-lint` step：tag 含 `-` 必须匹配 `v\d+\.\d+\.\d+-(alpha|beta|rc)\.\d+`，否则失败 | 0.2 天 | 一次故意打错 tag 触发失败 |
| R2 | release.yml 加 minisign / cosign 签名 step + updater 端校验（GH 账号被入侵兜底，`auto-update.md:140` 自陈缺口） | 1 天 | `updater::apply` 加 minisign verify 后通过测试 |
| R3 | 升级失败自动回滚增强：fork-exec 后 60s 内子进程 `GET /health` 不 200，父进程回滚 `wordforge.{old_tag}` | 1 天 | 集成测试故意启动子进程 panic |
| R4 | 自更新 apply 进入 swapping 时自动开 maintenance 模式，failed/completed 时关闭 | 0.5 天 | 集成测试观察 503 响应窗口 |

### 6.3 文档（P0）

| # | 项 | 工时估 | 验收 |
|---|---|---|---|
| D1 | 项目根加 `README.md`：项目简介 + 安装 + 快速使用 + 文档站链接 + 许可证 | 0.5 天 | GitHub 仓库首页可见 |
| D2 | 加 `CHANGELOG.md`：从 v0.1.2 到 v0.6.0-beta.4 全量；脚本化（解析 GH release notes 落地） | 1 天 | 仓库根可读 |
| D3 | 加 `SECURITY.md`：安全披露通道 + 漏洞响应窗口 | 0.2 天 | 顶级文件存在 |
| D4 | 加 `CONTRIBUTING.md`：Conventional Commits + PR 模板 + 本地跑测试三件套（`cargo test` + `npm test` + `npm run test:e2e`） | 0.5 天 | 顶级文件存在 |
| D5 | 更新 `docs/auto-update.md` 双通道 + `channel` 字段 + 当前 v0.6.0-beta.3 实际数据流（异步 apply + applyTask） | 0.5 天 | doc 与 `updates.rs` 对齐 |
| D6 | 把 `auto-update.md / alignment.md / amas-schema-codegen.md / openapi.yaml / ui-audit.md` 加入 vitepress sidebar 的"运维"和"开发者参考"分类 | 0.2 天 | 文档站 nav 完整 |
| D7 | 写 `docs/runbook/` 子目录：backup-restore.md、incident-response.md、key-rotation.md、scaling.md、monitoring-setup.md | 3 天 | 5 篇 runbook 落地 + sidebar 收录 |
| D8 | 写 `docs/user/` 子目录给最终用户（不止开发者）：installation-ios.md、installation-web.md、faq.md、privacy.md | 2 天 | 4 篇用户文档落地 |

### 6.4 运维 / 监控（P1）

| # | 项 | 工时估 | 验收 |
|---|---|---|---|
| O1 | 暴露 `/metrics`（OpenMetrics 文本格式，axum-prometheus 集成）：QPS / 延迟 P50/95/99 / SSE 连接数 / DB 大小 / worker last_run_at | 2 天 | curl `/metrics` 拿到 prometheus 可抓内容 |
| O2 | nginx sample.conf + TLS（certbot）runbook | 1 天 | `docs/runbook/nginx.md` |
| O3 | DB 备份外迁脚本（rsync / S3 二选一）；cron 整合到 install.sh 选装 | 2 天 | `scripts/backup-to-s3.sh` + runbook |
| O4 | maintenance 模式 admin UI 开关（`/admin/settings` 现有面板加切换） | 0.5 天 | 前端 toggle 落地 + e2e |
| O5 | 升级历史审计表 `update_audit_log`（migration） | 0.5 天 | 表落地 + admin UI 展示 |

### 6.5 多实例 / 高可用（P2，可推到 v1.1）

| # | 项 | 工时估 | 验收 |
|---|---|---|---|
| H1 | leader 选举说明（当前已经有 `WORKER_LEADER=true` env，但 runbook 缺）；多实例并行写 SQLite 的隔离规则 | 2 天 | `docs/runbook/multi-instance.md` |
| H2 | 灰度发布：min_client_version + feature flag 在 admin UI 上的可视化 | 3 天 | 前端面板 + 集成 |
| H3 | DB 从 SQLite 切到 PostgreSQL 的迁移路径（可选） | 长期 | 评估文档 |

---

## 7. 关键引用速查

| 文件 / commit / PR | 作用 |
|---|---|
| `release.yml:105` | prerelease 规则（`contains(github.ref_name, '-')`） |
| `updater.rs:165-181` | UpdaterCache stable + beta 双通道 |
| `updater.rs:820-880` | parse_release_list_payload 双通道分流 |
| `updates.rs:25-30` | admin/updates 三件套 router |
| `updates.rs:104-207` | 异步 apply + tokio::spawn 不阻塞 handler |
| `state.rs:25-84` | SseEvent 9 个变体定义 |
| `middleware/strict_mode.rs:40-47` | strict-mode 豁免 admin/v1/status/SSE |
| `install.sh:99` | systemd `Restart=on-failure`（坑点 4） |
| `alignment.md:144-147` | 第三轮对齐 0 P0 / 0 P1 |
| `frontend/src/types/admin.ts:89-124` | AdminUpdateStatus / ChannelStatus / ApplyTaskStatus |
| `docs/auto-update.md:102-129` | 手动回滚 runbook（仅有的故障 runbook） |
| `docs/api-endpoints.md:2353-2371` | SSE 事件表（缺 4 个） |
| `docs/openapi.yaml:4` | version: 0.4.3（落后 4 个 release） |
| `routes/v1.rs:1-11` | V1 兼容层绕过 AMAS 警告 |
| `routes/admin/monitoring.rs:102-178` | 遗留单通道 check-update 接口 |
| commit `d0325f8` | WordState wire 序列化 lowercase（breaking） |
| commit `fb93944` | favorites paginated()（hotfix） |
| commit `ab37387` | v0.6.0-beta.3 双通道 |
| commit `49f11ac` | v0.6.0-beta.2 FeedbackPage ErrorBoundary 修 |
| commit `7682383` | v0.5.0 release（自更新里程碑） |

---

## 8. v1 GA 前阻塞性结论（一句话）

**契约层四轮对齐已稳定（0 P0），发布流双通道已闭环，但「自更新失败保护机制」「外部可抓监控」「最终用户文档」「README/CHANGELOG/SECURITY 三件套」缺失程度足以让 v1 stable 公告失血。建议在 GA 前完成 §6 中 P0 全 14 项；P1 / P2 可在 GA 后窗口期补。**
