# WordForge v1.1.3 Backlog

> 起草日期：2026-06-02
> 来源：盘点工作流（5 线挖掘 → 逐项回代码库对抗式核验 → 归并定级，33 agent）
> 颗粒度：每条 ≤ 2 人日为宜（超出已注明拆分点）；可直接拆为 GitHub issue。
> 字段：**维度** = category（arch-promise / perf-ops / client-coordination / honest-downgrade-followup / beta-followup / code-debt / deferred-backlog）；**优先级** = P0（阻断/紧迫）/ P1（重要债·契约风险）/ P2（可选优化·UX 收尾·代码债）；**估时** = 核验后修正值（单人日，含测试与文档）。
> ⚠️ 每条「实施陷阱」由核验阶段回代码库求证得出，是规划时点判断之外的真实修正，落地前务必读。

## 版本定位

当前最新 `v1.1.2-beta.4`（分支 `feat/admin-ui`，领先 `main` 94 commits）测通后**先原样转正为 v1.1.2 stable（GA）**。**v1.1.3 是 v1.1.2 之后的下一个增量版本**，非 beta 转正，收纳当前所有 beta 已实现内容之外、仍欠的债与配套闭环。

经用户拍板，v1.1.3 取**切法 B —— 含 S2 的功能 minor**（全量 19 项），而非纯加固 patch。

## 总览

| 里程碑（执行波次） | 主题 | 任务数 | 估时 | 优先级分布 |
|---|---|---|---|---|
| **W1 遥测契约协同** | beta.4 破坏性遥测契约的客户端协同 + 本仓自伤修复 | 2 | 4.0 人日 | P0×1 / P1×1 |
| **W2 性能运维加固** | 落地 6-01 压测两条根因 + 灾备外迁 | 5 | 5.5 人日 | P1×2 / P2×3 |
| **W3 架构承诺 S2** | records → AMAS 真异步 + outbox（兑现 v1.1 承诺） | 1 | 5.0 人日 | P1×1 |
| **W4 功能补全 / 降级跟进** | honest-downgrade 可演进项 | 4 | 13.0 人日 | P2×4 |
| **W5 前端配套 + 代码债收口** | 低成本搭车清理 | 7 | 2.9 人日 | P2×7 |
| **合计** | | **19** | **≈ 30.4 人日** | P0×1 / P1×4 / P2×14 |

---

## W1 · 遥测契约协同（beta.4 破坏性契约闭环）

> beta.4 遥测硬识别是**无灰度开关、上线即生效**的破坏性契约，生产已是 beta.4，断流风险**可能正在发生**。本波最高优先。

### T1 · 遥测 `device.model` 客户端协同 + 本仓 admin-ui 自伤修复
- **维度**：client-coordination
- **优先级**：**P0**
- **估时**：1.5 人日
- **依赖**：m038（已落 beta.4）
- **描述**：beta.4 把 `payload.device.model` 设为新增必填字段，缺失直接 `400 MISSING_DEVICE_MODEL`。拆三块：① **本仓即时修复（必做、不依赖跨仓，~0.3d）**——本仓 admin-ui 自身遥测 worker 会撞 400，给 `DeviceFingerprint` / `collectDeviceFingerprint()` 补 `model` 字段（Web 端无可靠真型号，落 `browserName+osName` 派生标识或固定 `web-admin` 占位）；② **补契约文档（~0.2d）**——`docs/api-spec.md` 遥测 payload 段标注 `device.model` / `device.timezone` 为新增必填及 400 码；③ **跨仓协同（~1d，沟通/排期非本仓编码）**——与 `wordforge-web` 维护者对齐上报 `device.model` 后再发版。
- **验收**：本仓 admin-ui session_start 遥测上报含 `device.model`、不再撞 400；`docs/api-spec.md` 契约段更新；`wordforge-web` 上报体补 model 并约定最低后端版本（登记 `docs/release-calendar.md`）。
- **出处/证据**：`src/routes/telemetry.rs:159-170`（后端校验已实现）；`admin-ui/src/workers/telemetry.ts:154` + `admin-ui/src/lib/device.ts:38-51,100-116`（fingerprint **无 model 字段**=本仓自伤）；`docs/api-spec.md` grep `device.model` 零命中。
- **⚠️ 实施陷阱**：原候选定性为「纯外部对接、本仓无改动」**是错的**——本仓 admin-ui 自身遥测当前就会撞 400，这是 beta.4 引入的本仓内可直接修复的自伤回归，先于跨仓协同处理。

### T2 · 遥测四要素硬校验与客户端契约端到端联调
- **维度**：client-coordination
- **优先级**：P1
- **估时**：2.5 人日
- **依赖**：T1（device.model 上报落地）
- **描述**：遥测除 model 外还强制平台 header（`x-device-platform`）、版本 header（`x-app-version`）、`payload.device.timezone`，缺任一即 4xx；并三态归属核验（`owner=None`→403 `DEVICE_NOT_REGISTERED`、`owner≠me`→403 `DEVICE_OWNERSHIP_MISMATCH`、`owner=NULL`→claim 放行）。无灰度开关，须跨端联调验收 + 补本仓可交付的两项低成本物：(a) 补 `tests/telemetry_http.rs` 五个拒绝码负路径断言（作 spec-of-record）；(b) 修正已过期的客户端契约文档。
- **验收**：`tests/telemetry_http.rs` 覆盖 5 个拒绝码负路径（MISSING_DEVICE_MODEL / MISSING_TIMEZONE / MISSING_APP_VERSION / DEVICE_NOT_REGISTERED / DEVICE_OWNERSHIP_MISMATCH）；各客户端 header/payload 载体齐全、NULL-claim 老匿名设备认领不误伤、403 场景客户端有合理降级；过期文档已对齐。
- **出处/证据**：`src/routes/telemetry.rs:128-170,214-233`（后端已实现）；`src/middleware/device.rs:73-78,156`（telemetry skip upsert 时序修复）；`tests/telemetry_http.rs:43-233`（**无负路径断言**，全 happy path）；`docs/v1-client-migration.md:235,259` + `docs/api-spec.md:100`（**仍按旧软门控描述、与硬校验冲突**）。
- **⚠️ 实施陷阱**：`docs/v1-client-migration.md` 仍把 timezone 写成 strict-mode 软门控、未记录新增必填 model 与两个 403 码；`docs/api-spec.md:100` 仍写 `x-device-platform` 默认 unknown，与新「缺失即硬 4xx」矛盾——这两处文档过期是联调对齐的盲点。

---

## W2 · 性能运维加固（落地 6-01 压测两条根因 + 灾备）

> 2026-06-01 压测两条根因，beta.3 只落了 SQLite `cache_size` 这一半。本波补齐另一半，多为配置/文档/一行默认值，**零发版二进制风险**。

### N1 · SQLite 连接池默认值 16 → 8
- **维度**：perf-ops
- **优先级**：P1
- **估时**：0.5 人日
- **依赖**：无
- **描述**：峰值内存 = SQLite page cache × pool 连接数。beta.3 已改 `cache_size`（-64000→-16000），但 `SQLITE_POOL_SIZE` 默认仍 16，prod `.env` 未设走默认。两因子相乘才决定上界：当前 15.6MiB×16≈250MB，降 pool 到 8 才能向压测目标 ~100MB 收敛。
- **验收**：`src/config.rs:330` 默认值 16→8；`.env.example:16` 同步；CHANGELOG 记录；与 `connection_timeout`（250ms）做一次轻量回归确认 pool 缩小不致 503。
- **出处/证据**：`src/config.rs:330`（默认仍 16）；`src/store/mod.rs:80-81`（cache_size 已收紧）、`:72-73`（注释已点明 ×pool 关系但未动 pool）；`memory/perf_loadtest_findings_2026_06_01.md:13,16`。
- **⚠️ 实施陷阱**：降到 6 还是 8 需斟酌——goodput 不变前提下兼顾并发吞吐，建议取 **8** 留余量。

### N2 · nginx named upstream + keepalive 64 + `map $http_upgrade`（SSE 保留 upgrade）入运维样例
- **维度**：perf-ops
- **优先级**：P2
- **估时**：0.25 人日
- **依赖**：无
- **描述**：生产 nginx 直连 `proxy_pass http://127.0.0.1:3000` 无连接复用，空闲挂 194 TIME-WAIT。统一两份样例为 named upstream + `keepalive 64` + `proxy_http_version 1.1` + `Connection ""`，并用 `map $http_upgrade $connection_upgrade` 让 SSE 端点保留 upgrade。**不提 goodput，省 TIME_WAIT/稳定性**。
- **验收**：两份样例均含 named upstream + keepalive 64 + map 块；非 SSE 路径走 keepalive、SSE 端点保留 upgrade；`deploy/nginx/*.conf*` grep `connection_upgrade` 命中。
- **出处/证据**：`deploy/nginx/sample.conf:17-20`（keepalive 仅 **32** 非 64）、`:32-65`（location 缺 http_version/Connection）；`deploy/nginx/wordforge.conf.sample:63,84,101,111`（4× 直连无 named upstream）；全仓 `map $http_upgrade` 零命中。
- **⚠️ 实施陷阱**：两份文件缺口不同，勿照搬同一 patch——`sample.conf` 两头都缺（无 http_version 也无 Connection）；`wordforge.conf.sample` 的非 SSE 块已带 http_version 1.1+Connection ""，**只缺 named upstream 与 keepalive 指令本身**。

### N3 · sysctl `tcp_max_tw_buckets`→262144 + nginx `worker_connections` 上调入 runbook
- **维度**：perf-ops
- **优先级**：P2
- **估时**：0.75 人日
- **依赖**：N2（与 keepalive 配套：keepalive 减 TIME_WAIT 产生量、tw_buckets 兜底突发）
- **描述**：压测实测 `tcp_max_tw_buckets=5000` 被打爆（TcpExtTW 162797）、`worker_connections 768` 偏低。内核/nginx 全局参数须固化进部署 runbook（`sysctl.d` + nginx `events{}` 块），否则换机重装即回退默认。
- **验收**：新增 `sysctl.d` 样例（`net.ipv4.tcp_max_tw_buckets=262144`）+ nginx `events{ worker_connections … }`；`docs/runbook/` 补「内核/nginx 抗突发参数」段。
- **出处/证据**：`deploy/`、`docs/runbook/` grep `tw_buckets`/`worker_connections`/`sysctl` 零命中；`docs/runbook/scaling.md` 只讲 SQLite pool/容量不涉内核；`memory/perf_loadtest_findings_2026_06_01.md:21-22`。
- **⚠️ 实施陷阱**：`worker_connections` 是 nginx `events{}` 块全局指令，两份样例目前**都不含 `events{}` 块**，需新增。

### N4 · systemd unit 增设 `MALLOC_ARENA_MAX=2`（glibc arena 碎片次因兜底）
- **维度**：perf-ops
- **优先级**：P2
- **估时**：0.25 人日
- **依赖**：N1（主修复 cache_size+pool）
- **描述**：压测把 glibc arena 碎片定性为内存**次因**（主因是 SQLite pcache，已由 beta.4 cache_size 收口）。`MALLOC_ARENA_MAX=2` 收敛多线程下每线程 arena 数，与 pool/cache_size 主修复正交互补。**勿当主推**。
- **验收**：`deploy/wordforge.service.tmpl` [Service] 段加 `Environment=MALLOC_ARENA_MAX=2`。
- **出处/证据**：`deploy/wordforge.service.tmpl:14-29`（无该项）；全仓 grep `MALLOC_ARENA_MAX` 零命中。
- **⚠️ 实施陷阱**：生产 8.135.57.148 已落地的 `wordforge.service` 需在升级时同步该 Environment 才生效（install.sh 重写 unit 或 SSH 手改），**勿只改模板就认为生产已收益**。

### B1 · DB 备份外迁到外部存储（S3 / rsync 离站）
- **维度**：deferred-backlog（灾备）
- **优先级**：P1
- **估时**：3.0 人日
- **依赖**：现有 `db_backup` 本地备份 worker + `BackupTarget` 配置 schema
- **描述**：当前每日备份仅本地文件系统拷贝 + prune，单机磁盘损坏即全丢、无离站副本。`BackupTarget.uri`（`s3://` / `glacier://`）配置 schema 与校验已就位但**无任何消费方**。实现按 `BackupTarget.uri` scheme 分发上传（S3/rsync/file）+ 远端保留策略 + 失败告警接入 `system_alerts`。
- **验收**：`tests/` 含 mock S3 / 本地 rsync target 集成测试；每日备份成功推送到配置的远端；上传失败触发 system_alerts；`docs/runbook/backup-restore.md` 补离站章节。
- **出处/证据**：`src/workers/db_backup.rs:1-55`（仅 std::fs，无上传）；`src/main.rs:295-315`（每日循环仅传本地 dir，不读 targets）；`src/routes/admin/settings_sections.rs:530-563`（schema+validate 但无消费方）；`Cargo.toml` 无任何对象存储依赖；`docs/v1-research/backlog.md:426`、RFC §4.3 W10。
- **⚠️ 实施陷阱**：需新增对象存储依赖（`object_store` 或 `aws-sdk-s3`）选型 + 鉴权凭据注入路径（env / settings？）+ 错误处理，原 1.5 天估值偏乐观，按 3 天更现实。

---

## W3 · 架构承诺 S2（兑现 v1.1 核心承诺）

### S2-1 · records → AMAS 真异步消费 + outbox 持久化（消除手动 rollback 非原子性）
- **维度**：arch-promise
- **优先级**：P1
- **估时**：5.0 人日
- **依赖**：已落地的内存事件总线基础设施（`event_bus.rs`，default feature 已开）
- **描述**：v1.1 核心架构承诺，姊妹项 S1 已兑现、唯 S2 仍欠债。当前仅落了事件总线**基础设施**（内存 broadcast + `RecordCreated` 旁路 emit + 计数 consumer），AMAS 仍走 handler 内同步 `process_event` + 手动 rollback 主路径。补齐四件：① `outbox` 表持久化（重启不丢事件）② AMAS 真异步消费替换同步主路径 ③ `events_dead_letter` 表 + 重试退避 ④ admin `/admin/monitoring` 加 outbox lag / 死信计数。完成后去掉 `records/single.rs` 与 `batch.rs` 的手动 rollback。
- **验收**：`outbox` + `events_dead_letter` 迁移落地；feature flag 渐进切换（兼容老同步路径）；集成测试覆盖「重启不丢事件」与「死信路径重试」；`single.rs`/`batch.rs` 手动 rollback 删除后回归全过；admin 监控可见 outbox lag / 死信数。
- **出处/证据**：`src/services/event_bus.rs:7-10`（自述 outbox/真异步留给后续 task）、`:116-177`（consumer 仅 received/lagged 计数）；`src/routes/records/single.rs:293-314,164-238,412-424`；`src/routes/records/batch.rs:183,287-297,306-307`；全仓 `outbox`/`dead_letter` 仅命中 3 条「为未来铺路」注释；`docs/v1-research/should-deferred.md §S2`、RFC §4.2 / R06。
- **⚠️ 实施陷阱**：兑现时**顺带修正 `CHANGELOG.md:359-361`** 对 rc.2「AMAS engine 异步消费」的夸大表述——当时实为计数旁路 tap，与代码不符。不定 P0：现状是「单事务 + best-effort rollback」，RFC R06 风险等级 M，生产已稳定跑多个 beta，属重要架构债非阻断；4 天偏紧故上调 5 天。

---

## W4 · 功能补全 / honest-downgrade 跟进

> 均为**临时降级的可演进项**（非永久架构降级、非 v2 级），基建已就绪。

### D1 · 为 admin 补独立应用内通知 / 告警收件箱
- **维度**：arch-promise
- **优先级**：P2
- **估时**：4.0 人日
- **依赖**：无
- **描述**：beta.4 AMAS 软拦截告警因「admin 无应用内通知箱」只能落 `system_alerts` 表 + 经 `/admin/monitoring` 派生式透出，无 SSE、无主动推送，admin 须主动轮询监控页才看得到。补 admin 独立通知存储 + 读/标记已读路由 + 可选复用 SSE 推送 + admin-ui 收件箱组件与未读角标。
- **验收**：admin 可在收件箱主动收到 AMAS 告警；未读角标；标记已读持久化；告警不再仅靠轮询监控时间线。
- **出处/证据**：`src/services/alerting.rs:1-61`（双通道，admin 无通知）；`src/routes/admin/monitoring.rs:482-516`（派生式轮询，注释「无独立告警表」）；`src/store/schema.rs:22-31`（admins 表无通知列）、`:104-117`（notifications PK 按 end-user user_id 键控）；`admin-ui/src/api/notifications.ts`（指向 end-user 路由，全仓零消费）。
- **⚠️ 实施陷阱**：**绝不给 admin 复用 end-user `create_notification`**（`notifications` 表按 user_id 键控，与 admin 维度不同）。可给 admins 维度新表，或给 `system_alerts` 加 `read/ack` 状态列 + 迁移。

### D2 · 设备推送：补「投递时机调度」与「草稿存储」（当前 disabled 占位）
- **维度**：honest-downgrade-followup
- **优先级**：P2
- **估时**：4.0 人日
- **依赖**：无
- **描述**：设备页推送编辑器「投递时机（立即/延时/指定时间）」「保存草稿」两控件是 disabled 占位，设计稿 `clients.html:379-404` 作可用功能。两项不依赖外部 push 厂商、纯后端可落（`scheduled_broadcasts` 表 + 定时 worker；draft 复用 `feedback_reply_drafts` 表式范式）。
- **验收**：投递时机可选延时/指定时间并真实定时下发；草稿可存可取；前端控件解除 disabled；e2e 覆盖。
- **出处/证据**：`admin-ui/src/pages/DevicesPage.tsx:1041-1074`（disabled 占位）；`src/store/migrate.rs:1842-1851`（broadcasts 表无 scheduled/draft 列）；`src/routes/admin/broadcast.rs:203-320`（纯即时 fan-out）；基建 `src/workers/mod.rs:30` tokio_cron_scheduler 在用、`src/main.rs:305` 已有 daily-backup interval loop 范式。
- **⚠️ 实施陷阱**：**不含 APNs/FCM 多渠道勾选**（`DevicesPage.tsx:1055-1058`，需外部 provider，继续保持 disabled，勿一并排入）。

### D3 · 持久化可用率滚动存储，让登录页「SLO 30d」可达真实 30 天口径
- **维度**：honest-downgrade-followup
- **优先级**：P2
- **估时**：2.0 人日
- **依赖**：无
- **描述**：登录页 SLO 卡已做诚实降级（按真实窗口动态标注），但根因仍在——可用率数据源是纯内存 `RollingStore`，`HOUR_CAP=7*24` 仅 7 天且重启清零，永远到不了设计稿「SLO 30d 99.95%」。补一张按小时落 SQLite 的可用率 rollup（5xx/total 聚合），重启可恢复、窗口达 30 天。
- **验收**：新增 `availability_rollup` 表（或复用 `monitoring_timeseries` period 命名空间）；启动回灌 hour 桶 + `first_record_unix`；每小时 flush（可挂现有 `metrics_flush` worker 旁路）；`HOUR_CAP` 提到 30*24 且 aggregate 优先读持久层；登录页 SLO 30d 真实点亮。
- **出处/证据**：`src/middleware/http_metrics.rs:201-202,249-255,281-283,381-384`（7d 内存态、重启清零）；`src/routes/health.rs:119-134`（SLO 直接 `aggregate(30*24*3600)` 但被 7d 窗口卡死）；`admin-ui/src/pages/LoginPage.tsx:42-54`（诚实降级已落地）；schema 全表无可用率 rollup 表。
- **⚠️ 实施陷阱**：勿混淆既有的 `metrics_persistence.rs`/`algorithm_metrics_daily`（落的是 AMAS 6 算法引擎指标）与本项 HTTP 可用率——二者无关。数据结构（hour 桶/aggregate）现成，原估 3 天偏高，下修 2 天。

### D4 · 灰度发布收窄为 `min_client_version` 版本门控的 admin UI 化
- **维度**：client-coordination
- **优先级**：P2
- **估时**：3.0 人日
- **依赖**：无
- **描述**：仓内已有的 canary 是 **AMAS 引擎配置灰度**（决定走哪套引擎参数），非发布/二进制按版本切流。真正可落的窄切片（release-keeper §6.5 H2）：把当前仅 env 的 `min_client_version` 做成 admin 运行时可配 + feature-flag 可视化面板。
- **验收**：admin 可在 UI 运行时设置 `min_client_version` / 版本门控开关并即时生效；strict-mode 按新配置拒绝旧客户端（`CLIENT_OUTDATED`）；集成测试覆盖。
- **出处/证据**：`src/store/operations/amas_canary.rs:7-10`（canary=引擎参数灰度非发布）；`src/config.rs:69-74,411`（`min_client_version` 仅 env `MIN_CLIENT_VERSION`）；`src/routes/admin` 无任何运行时设置端点；`docs/v1-research/04-release-keeper.md:334`（H2）。
- **⚠️ 实施陷阱**：**勿复用 amas_canary 的 percent/crowd-filter 做发布百分比灰度**——AMAS canary 改不了「发布/二进制/客户端可用版本」切流；真正的百分比发布灰度需先有多实例+服务发现（W4/H1，仍延后），不在本项范围。需**新增 strict-mode 运行时写端点**而非纯复用，原 2 天低估，上调 3 天。

---

## W5 · 前端配套 + 代码债收口（低成本搭车）

### E1 · 广播受众过滤 400 错误码前端配套提示
- **维度**：beta-followup
- **优先级**：P2
- **估时**：0.5 人日
- **依赖**：无
- **描述**：beta.1 后端把广播「静默零发送」改为返回 400（非法 version_min→`INVALID_VERSION_MIN`、过滤后 0 人→`EMPTY_AUDIENCE`），但前端从未配套，发送失败只弹通用「发送失败」、预估命中 catch{} 静默吞错显「—」。做错误码→中文提示映射 + versionMin 输入加 semver 校验。
- **验收**：填错版本号/受众 0 人时显示具体可操作原因；versionMin 前端 semver 预校验把 `INVALID_VERSION_MIN` 拦在请求前；preview catch 区分「非法版本号」与真实网络失败。
- **出处/证据**：`src/routes/admin/broadcast.rs:178,240`（两码）；`admin-ui/src/pages/DevicesPage.tsx:550-554,582-586,1015-1021,1036-1037`。
- **⚠️ 实施陷阱**：候选标题把页面记错——真正要改的是 **`DevicesPage.tsx`**（受众过滤 UI 所在），**不是 `BroadcastPage.tsx`**（全员广播页，只发 title/message，触发不了这两个码）。

### E2 · 广播历史「本周/失败」筛选下推后端跨页查询
- **维度**：honest-downgrade-followup
- **优先级**：P2
- **估时**：1.0 人日
- **依赖**：无
- **描述**：week/failed 筛选目前只在前端对「当前分页页」做 filter（页脚已注释声明此限制）。把筛选下推后端：`list_broadcasts` 增 filter 参数 + WHERE（failed = `sent_count=0`；week = `created_at >= now-7d`）+ count 同步过滤。
- **验收**：筛选作用于全部历史而非当前页；分页 total 按 filter 后计数正确；KPI 卡仍为近 30 天全量不随 filter 变。
- **出处/证据**：`admin-ui/src/api/admin.ts:229`、`src/routes/admin/broadcast.rs:24-29`、`src/store/operations/broadcasts.rs:45-58`、`admin-ui/src/pages/BroadcastPage.tsx:141,155-163`。
- **⚠️ 实施陷阱**：真正的工程点是 **`pagination.total` 当前复用 `stats.total`，加 filter 后必须改为「按 filter 过滤后的计数」**，否则页脚分页数仍错；别只加 WHERE 忘了 count。原估 2 天偏高，下修 1 天。

### E3 · canary 自动回滚阈值改 `system_settings` 可配（C6 收尾）
- **维度**：code-debt
- **优先级**：P2
- **估时**：0.75 人日
- **依赖**：无
- **描述**：`canary_monitor` 的 `REWARD_DROP_THRESHOLD` / `ANOMALY_RISE_THRESHOLD` 写死常量 0.05，带 `TODO(C6)`。迁到 `system_settings` 让 admin 在线调参，免改代码重发版。
- **验收**：两阈值落 `SystemSettings` + DB 列 + get/save SQL + 端点 + admin-ui SettingsPage 表单项；阈值 0~1 范围校验。
- **出处/证据**：`src/workers/canary_monitor.rs:14-18`（TODO+常量）；`src/store/operations/system_settings.rs:9-31,83-86`（无 canary 阈值字段）；`admin-ui/src/pages/amas-advisor/PatchCanaryCard.tsx:58`（仅手动回滚按钮）。
- **⚠️ 实施陷阱**：照 `amas_auto_apply_min_confidence` / `llm_advisor_max_cost_per_month_yuan` 两个现成 f64 配置项的全链路（结构体→DB 列→端点→SettingsPage）画瓢即可；跨后端 5 处 + 前端 1 处，原 0.5 天偏紧上调 0.75。

### E4 · 前端拆 `vendor-echarts` / `vendor-codemirror` 独立 chunk
- **维度**：perf-ops
- **优先级**：P2
- **估时**：0.5 人日
- **依赖**：无
- **描述**：`vite.config.ts` manualChunks 仅含 vendor-solid/router/mediapipe。在 manualChunks 加 `'vendor-echarts': ['echarts']` 与 `'vendor-codemirror': [按需 @codemirror 子包]` 两条。
- **验收**：build 产物含两个命名 vendor chunk；ScriptEditor/TomlEditor、各图表面板懒加载仍正常。
- **出处/证据**：`admin-ui/vite.config.ts:30-34`；`admin-ui/src/components/ui/EChart.tsx`、`TomlEditor.tsx`、`ScriptEditor.tsx`。
- **⚠️ 实施陷阱**：**价值是「跨路由 chunk 去重 + 长期缓存稳定」，不是「减真首屏体积」**——真首屏 LoginPage 本就不含这两库，已被 App.tsx 路由级 `lazy()` + Rollup 自动分割隔离（echarts 552KB / codemirror 282KB 已是按需 chunk）。EChart.tsx 用 `echarts/core` 子路径，配 chunk 时匹配 `'echarts'` 以保去重生效。

### E5 · v1 弃用响应迁移文档 URL 替换占位常量
- **维度**：code-debt
- **优先级**：P2
- **估时**：0.5 人日
- **依赖**：无
- **描述**：`/api/v1/*` 弃用中间件返回的 `V1_LINK_URL` 指向 `https://docs.wordforge.app/api/v1-deprecation`，是占位。改为真实 GitHub Pages URL + 锚点。
- **验收**：`V1_LINK_URL` 指向真实可达链接（如 `https://heartcoolman.github.io/wordforge/api-endpoints#…`）；同步 `src/routes/v1.rs:5` 注释与 `docs/api-endpoints.md §18` 内同字符串；链接核实可达。
- **出处/证据**：`src/middleware/deprecation.rs:64,67`；`src/routes/v1.rs:32,44`；`gh api .../pages` 确认真实站点 `heartcoolman.github.io/wordforge/`、`docs.vitepress base:'/wordforge/'`。
- **⚠️ 实施陷阱**：**根因不是「文档站未部署该锚点」而是常量指向了压根不存在的域名** `docs.wordforge.app`（真实站点是 `heartcoolman.github.io/wordforge/`）——**现在就能改对**，无需等文档站。

### E6 · admin 登录页语言切换控件移除（而非接入 i18n）
- **维度**：code-debt
- **优先级**：P2
- **估时**：0.2 人日
- **依赖**：无
- **描述**：`LoginPage` 顶部语言切换是纯视觉占位（locale 信号不驱动界面语言），admin-ui 全站无 i18n。admin 是后端内嵌运维面、面向单语运维，**全站 i18n 无产品收益（v2 级）**。最经济收口是删除该误导控件。
- **验收**：删除 `LoginPage.tsx:24` 的 locale 信号与 `:341-352` 的 select；lint/e2e 全过；无功能损失。
- **出处/证据**：`admin-ui/src/pages/LoginPage.tsx:24,341-352`（控件自带 aria-label「暂未生效」/title「暂未接入」=开发者本人也知是占位）。
- **⚠️ 实施陷阱**：候选原写「接入最小 i18n 框架（1.5d）」**是错的方向**——全量 i18n 要抽 118 个 .tsx 硬编码中文 + 引框架，是 v2 级。本项只做「移除控件」（0.2d）。若产品确有 admin 多语言路线图，单独按 v2 立项。

### E7 · 修复或废弃 `build-changelog.sh`
- **维度**：code-debt
- **优先级**：P2
- **估时**：0.2 人日
- **依赖**：无
- **描述**：脚本 python 段缺 `>> "$OUT"` 重定向，正文 print 全打终端从不写文件，跑全量重建会把 CHANGELOG 清成只剩头部（2026-06-02 已实际踩坑）。且即便修好也不可用——GitHub Release body 不含手写中文复盘。脚本头注释「全量重建时重跑」与 CHANGELOG 头部「勿用脚本全量重建」自相矛盾。
- **验收**：直接删除脚本或标 `deprecated` 并修正头注释矛盾；CHANGELOG 维护流程文档一致。
- **出处/证据**：`scripts/build-changelog.sh:6,32,63`；`CHANGELOG.md:5`（与脚本 `:6` 矛盾）；`memory/changelog_maintenance_gotcha.md`。
- **⚠️ 实施陷阱**：修重定向也救不回——Release body 不含手写复盘，全量重建必摧毁这些内容。倾向直接删除 / deprecated。

---

## 明确不做（划界）

> 核验阶段剔除，**勿误捡进 issue**。

### 已被架构实际化解 / 前提不成立
- **vendor chunk「减首屏」论断不成立**：Rollup 自动分割 + 路由级 lazy 已把 echarts/codemirror 隔离成按需 chunk，不在入口包。本版仅保留 E4「命名去重」（价值是缓存稳定非减首屏）。建议把 `docs/v1-research/backlog.md:429` 标记为「已由 lazy 路由化解」。

### 仍未实现但本版无事可做
- **引导导览 waveOf/STEPS/e2e 同步**：v1.1.3 仍落在 'v1.1–1.2' 同波窗口，不重弹、e2e 不被拦，三处已自洽。真正触发点是未来跨入 ≥1.3 新波次。

### v2 级 / 范围不匹配
- **admin GUI 全量 i18n 多语言**：抽 118 个 .tsx 硬编码中文 + 引框架是 v2 级，运维面无产品收益（本版只做 E6 移除误导控件）。

### 残留占位但当前对运维不可见 / 价值低（可后续随手收，不单立 issue）
- `database_health` 的 `consecutiveFailures` 占位（S7 残留）——admin-ui `getDatabase()` 无调用方，死字段。
- probe worker IndexedDB count 写死 -1（M3）——仅 `/admin/remote-probe` REPL 读取，user-facing 影响零。
- 数据探针事件流升级真实 SSE——当前 5s 轮询已诚实标注，改需新建 telemetry→broadcast→SSE 链路，成本不匹配收益。
- AMAS 三个 disabled 预设落地 + 「另存为预设」——前后端均无 preset 支持，属新功能而非债。

### ⚠️ 刻意诚实降级，规划者勿误当 backlog 回退（7 项，均带代码注释标记为有意保留）
1. 探针 sink **永久裁剪**为真实 SQLite 表 + 派生探针（无 ClickHouse/Kafka/S3）
2. AMAS 甜甜圈 **6 个路由算法是正确口径**（设计稿「8」是语义错误，IAD/MTP/SSP 是记忆模型不参与路由）
3. resource-packs 三态 `installed/verify_failed/rollback` 对齐真实枚举
4. feedback 无投票数据源故不臆造票数
5. broadcast 真实 in-flight 状态非伪造逐批 SSE
6. analytics 无离开用户决策序列端点故不伪造
7. APNs/FCM 多渠道需外部推送厂商，继续保持 disabled

**这些不是待办，不应回退。**

---

## 附：优先级速查

| 优先级 | 任务 | 估时 |
|---|---|---|
| **P0** | T1 遥测 device.model 协同+本仓自伤 | 1.5d |
| **P1** | T2 遥测四要素联调 / N1 SQLite pool 16→8 / B1 DB 备份外迁 / S2-1 事件总线收尾 | 11.0d |
| **P2** | N2 nginx / N3 sysctl / N4 MALLOC / D1 admin 通知收件箱 / D2 设备推送调度 / D3 SLO 30d / D4 灰度版本门控 / E1 广播400前端 / E2 广播历史筛选 / E3 canary 阈值 / E4 chunk 拆分 / E5 v1 URL / E6 登录控件移除 / E7 build-changelog | 17.9d |
