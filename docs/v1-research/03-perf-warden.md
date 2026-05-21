# perf-warden — v1 性能 / 容量 / 稳定性基线

> 角色：perf-warden（性能守门员）  
> 日期：2026-05-21  
> 仓库：wordforge @ main（v0.6.0-beta.4）  
> 纪律：不登录生产服务器、不长跑 benchmark；仅盘点 + 推算 + 提案。

---

## 1. 现有 benchmark 清单

### 1.1 maimemo 离线评估管道（唯一一套真实数据基准）

代码：`benchmarks/maimemo/`  
入口：`python -m benchmarks.maimemo.cli {prepare|fit_oracle|evaluate|tune}`  
依赖：单独 venv（`uv venv .bench-venv --python 3.12` + `requirements.txt`），不进主构建。

**评估三层**（`benchmarks/maimemo/README.md`）：
- prediction：真实数据 logLoss / ICI / AUC / maeP / smapeH
- DHP reference：墨墨 SSP-MMC 模拟（expectedMemory / nextDayMemory / targetCount）
- interval policy：85% retention 目标下 safety / efficiency

**上次结果**（2026-05-15，`docs/amas-tuning-2026-05-15/01-final-report.md`，`~/.wordforge-bench/maimemo/reports/`）：

| 维度 | tune baseline | nearMiss[0]（已采纳） | 变化 |
|---|---|---|---|
| prediction_composite | 1.0000 | **1.1062** | **+10.62%** |
| ICI | 0.0508 | 0.0379 | -25.4%（校准更准） |
| logLoss | 0.5357 | 0.5340 | -0.32% |
| AUC | 0.6468 | 0.6453 | -0.23%（略降可接受） |
| DHP expectedMemory | 2765.4 | 3154.6 | **+14.1%** |
| DHP nextDayMemory | 2735.1 | 3103.6 | **+13.5%** |
| DHP targetCount | 739 | 640 | -13.4%（唯一退化） |

**oracle 基线**：`GRU-HLR oracle logLoss = 0.514 < HLR 0.625`，本工程 prediction logLoss 0.534 与 oracle 差距 0.020，已接近极限。

**重跑入口**（约 94 min/iter）：
```bash
source .bench-venv/bin/activate
python -m benchmarks.maimemo.cli evaluate --root "$WORD_FORGE_BENCH_DATA/maimemo"
# 完整 iter 1：baseline 6m + stage1 54m + stage2 10m + stage3 24m
```

### 1.2 内置算法运行时指标（非压测，是线上采样）

`src/amas/metrics.rs:8`：`LATENCY_BUCKETS = [100, 500, 1_000, 5_000, 10_000, ∞] µs`，对 6 个算法（heuristic / ige / swd / ensemble / mdm / mastery）各自维护 `AlgorithmMetrics`：call_count / total_latency_us / error_count / 6 桶直方图 + last_called_at。

可通过 `GET /api/admin/health/metrics`（`src/routes/health.rs:145`）读快照；`metrics_flush` worker `cron "0 */5 * * * *"`（5 min）持久化到 `metrics_daily` 表（`src/workers/metrics_flush.rs`）。

**桶分布的局限**：6 个 fixed bucket + 中点近似算 P50/P95/P99（`metrics.rs:62 bucket_midpoints`），精度有限，不适合做 SLO 卡控，**仅作 anomaly trigger**。

### 1.3 缺失的 benchmark（v1 前必须补）

- 路由级 HTTP 延迟（无 `tower-http` histogram，亦无 prometheus exporter）
- 端到端 SQLite query latency（`store/operations/*` 全部裸调用，无 instrumentation）
- 前端首屏 LCP / TTI（无 Lighthouse / Web Vitals 上报）
- 并发压力（无 wrk / k6 脚本，无负载形状定义）

---

## 2. 已知运行时配置（决定容量上限）

### 2.1 SQLite + 连接池

`src/config.rs:310-312`、`src/store/mod.rs:46-83`：

| 配置 | 默认值 | 备注 |
|---|---|---|
| `SQLITE_POOL_SIZE` | **16**（commit `2b80575`，原 4） | r2d2 max_size |
| `SQLITE_BUSY_TIMEOUT_MS` | 5000 | PRAGMA busy_timeout |
| `SQLITE_CONNECTION_TIMEOUT_MS` | 250（prod），2000（默认/测试） | r2d2 connection_timeout |
| `cache_size` | `-64000` = 64 MiB | PRAGMA per-conn |
| `mmap_size` | `268435456` = 256 MiB | PRAGMA |
| `temp_store` | `MEMORY` | PRAGMA |
| `journal_mode` | `WAL` | 必须 |
| `synchronous` | `NORMAL` | 性能/可靠折中 |

**.env.example 仍是 `SQLITE_POOL_SIZE=4`**（`.env.example:16`），与代码默认 16 不一致 → 生产 .env 若 copy 自 example 会被限制在 4，**待发布前检查**。

### 2.2 Tokio 阻塞池

`src/blocking.rs:43-57`、`src/main.rs:55`：

- 启动时 `init_blocking_semaphore(sqlite_pool_size as usize)` → permits = 16
- 所有 SQLite/算法重计算走 `run_blocking` → 全局信号量背压，**防 commit `134bcfe` 描述的运行时挂死**
- spawn_blocking 上限受 Tokio 默认 512 限制

### 2.3 HTTP / 中间件

`src/routes/mod.rs:53-129`、`src/main.rs:178-203`：
- 中间件栈（外→内）：CORS → Compression(gzip/br) → Trace → CatchPanic → 安全响应头 → request_id → device → strict_mode → maintenance → rate_limit
- `MAX_BODY_SIZE = 2 MiB`（`routes/mod.rs:37`）
- 无 `tower-http::timeout`，axum 自身不强制请求超时

### 2.4 限流

`src/middleware/rate_limit.rs`、`src/config.rs:327-334`：

| 限流域 | window | max | 默认 |
|---|---|---|---|
| 普通 API | `RATE_LIMIT_WINDOW_SECS=900` | `RATE_LIMIT_MAX=500` | 500 req / 15 min / IP |
| 认证路由 | `AUTH_RATE_LIMIT_WINDOW_SECS=60` | `AUTH_RATE_LIMIT_MAX=10` | 10 req / min / IP |
| 限流表上限 | — | `LIMITS_RATE_LIMIT_MAX_ENTRIES=100000` | 16 shard Mutex map |
| 限流清理 | — | `rate_limit_cleanup_interval_secs=300` | — |

IPv6 归并 `/64`（`normalize_ip_for_rate_limit`），仅基于 IP，**不带用户 ID**——共享出口的多人会互相影响。

### 2.5 SSE / 实时通道

`src/config.rs:120`、`src/routes/realtime.rs:60-75`：
- 全局上限 `LIMITS_MAX_SSE_CONNECTIONS = 1000`（CAS 卡 SSE_CONNECTION_COUNT）
- heartbeat_watchdog 每 5 s 扫描（`src/workers/heartbeat_watchdog.rs:8`），连续 5 次（25 s）缺心跳 → `DataCorrupted`
- AMAS 状态推送：`tokio::time::interval(...)`，每 tick 拿 `get_user_state_async`

### 2.6 Updater / 自更新

`src/services/updater.rs:208-220`：
- `client`（API/metadata）：30 s total timeout
- `download_client`：connect_timeout 15 s + read_timeout 60 s，**无 total timeout**（兼容国内到 GitHub 22 KB/s 慢链路）
- `max_tarball_bytes = 200 MiB`
- `update_checker` cron `0 0 */1 * * *`（每小时），可被 `UPDATE_CHECKER_INTERVAL_SECS` 配制但**目前 cron 写死**（`docs/auto-update.md:140`）

### 2.7 Worker cron

`src/workers/mod.rs:156-263`，21 个 worker（含禁用），核心高频：
- `delayed_reward` 5 min（从每分钟降下来的）
- `metrics_flush` 5 min
- `cache_cleanup` 10 min
- `monitoring_aggregate` 15 min（**WIP**，目前 enabled=false）
- `llm_advisor` 20 min（默认禁用）
- `update_checker` 整点（默认开）
- 凌晨 / 周一型聚合若干（`daily_aggregation` 01:00 / `health_analysis` 周一 05:00 / `weekly_report` 周一 06:30 / `confusion_pair_cache` 周日 05:00）

`add_job` 内部用 `AtomicBool guard` 跳过重叠运行（`workers/mod.rs:486-`），同一 job 不会并发执行。

### 2.8 release profile

`Cargo.toml:83`：`lto = true` + `codegen-units = 1` + `strip = true` —— 已开到顶；进一步榨需要 PGO，提升估计 < 5%。

### 2.9 前端 bundle

`frontend/vite.config.ts`：`manualChunks { vendor-solid, vendor-router, vendor-query, vendor-mediapipe }`，target `esnext`，`drop debugger / pure console.log` 已开。

**当前产物**（`static/assets/`，截至 2026-05-20）：
- 首屏组合：`index-*.js 231 KB` + `vendor-solid 27 KB` + `vendor-router 14 KB` + `vendor-query 24 KB` + CSS 69 KB ≈ **365 KB raw（gzip 估 ~110 KB）**
- 懒加载：`EChart 553 KB`、`ProbePage 414 KB`、`AmasConfigPage 97 KB`、其他 Admin 各页 8–20 KB
- 整 `static/assets/` 7.1 MiB（含 sourcemap）

**疲劳检测 wasm**：`crates/visual-fatigue-wasm` + `@mediapipe/tasks-vision`（mediapipe vendor chunk 在 build 时是 1 字节占位 → 真实模型仍是 lazy import）。

---

## 3. 关键路径 SLO 候选（P50 / P95 / P99 + 错误率 + 可用性）

> 置信度：H = 有 benchmark 数据 / M = 有间接证据（代码 / 配置 / commit log） / L = 纯推算。  
> 单实例 / 单 ECS 部署假设（8.135.57.148 Ubuntu 24.04 x86_64，记忆库 `wordforge_prod_deployment`）。  
> 没有任何路由级实测数据，下表所有数字均为**待压测验证**的提案值，不是已观测值。

### 3.1 后端 HTTP（user-facing）

| 路径 | 方法 | P50 | P95 | P99 | 错误率 | 置信 | 依据 |
|---|---|---|---|---|---|---|---|
| `/api/auth/login` | POST | 80 ms | 250 ms | 500 ms | < 0.5% | M | argon2 password hash 主导（CPU），后接一次 INSERT session；auth_rate_limit 60s/10 防爆破 |
| `/api/auth/refresh` | POST | 15 ms | 60 ms | 150 ms | < 0.3% | M | JWT verify + UPDATE session，无密码哈希 |
| `/api/learning/sessions` | POST | 60 ms | 200 ms | 400 ms | < 0.5% | L | 候选词排序（candidate_word_pool_size=500）+ AMAS state read + INSERT session |
| `/api/learning/sessions/:id/complete` | POST | 80 ms | 250 ms | 500 ms | < 0.5% | L | summary 落库 + AMAS 二次 read，事务规模小 |
| `/api/records`（单条） | POST | 40 ms | 120 ms | 300 ms | < 0.5% | L | get/set user_elo + word_elo + INSERT learning_records；走 `run_store_task` |
| `/api/records/batch` | POST | 200 ms | 600 ms | 1500 ms | < 1% | L | 视 batch 大小（max 500），事务化写 |
| `/api/words/batch-get` | POST | 30 ms | 100 ms | 250 ms | < 0.5% | M | 单事务前缀扫描（commit `e616047` 修了 N+1） |
| `/api/favorites?page=N` | GET | 25 ms | 80 ms | 200 ms | < 0.5% | H | paginated()，commit `fb93944` 修正 |
| `/api/v1/*` | * | + 5–15 ms | + 20 ms | + 40 ms | 同上 | M | 仅多一层结构转换；**v1 路由刻意绕过 AMAS**（commit `c758dda`），简单 CRUD |

### 3.2 后端 SSE / Admin

| 路径 | 指标 | 目标 | 置信 | 依据 |
|---|---|---|---|---|
| `/api/realtime/events` | 建连 ack < 500 ms | 2 s P95 | M | SSE_CONNECTION_COUNT CAS + DashMap insert，纯内存 |
| 同上 | event 推送延迟 | < 1 s P95 | M | tokio::interval tick + broadcast channel，与 worker 频率耦合 |
| 同上 | 连接保活 | ≥ 99% / 4h | L | 5 s heartbeat × 5 miss → 25 s 检测窗 |
| `/api/admin/updates/status` | P95 | < 100 ms | H | 纯缓存命中（cache_ttl 3600 s），不打 GitHub |
| `/api/admin/updates/check` | P95 | < 2 s | M | ETag 命中走 304；miss 走 GitHub API，与外网 RT 强相关 |
| `/api/admin/updates/apply` | 总时长 | < 90 s P95（不含下载） | M | 国内→GitHub release 实测 22 KB/s × 9 MB ≈ 7 min；以中性镜像（ghproxy）算 < 90 s |
| `/api/health` | P95 | < 50 ms | H | 静态 200 + db_ping 一次 SELECT 1 |

### 3.3 前端关键路径（按 100 Mbps 接入 + 中端 Android Chrome）

| 路径 | 指标 | 目标 | 置信 | 依据 |
|---|---|---|---|---|
| 首屏 `/login` | FCP | < 1.2 s | L | 365 KB raw / gzip ≈ 110 KB，强 cache 时直接命中；冷启动主导 |
| 首屏 `/login` | LCP | < 2.5 s | L | 同上 + theme-init.js + 字体 |
| 首屏 `/login` | TTI | < 3 s | L | esnext target，无 polyfill |
| 学习页 `/learning` | 路由切换可交互 | < 800 ms | L | 各 admin 页 lazy 8-20 KB；学习页未单独 chunk，并入主 index |
| 列表滚动（Vocabulary / WordbookList） | 60 fps（< 16 ms/frame） | 95% 帧 | L | SolidJS 反应式 + Pagination 默认 20/页；纯前端 |
| Admin Probe REPL | CodeMirror 首次加载 | < 1.5 s | L | ProbePage chunk 414 KB（含 CodeMirror）—— **首次进 admin probe 会有明显加载** |
| Admin Metrics 图表 | ECharts 首次加载 | < 1.5 s | L | EChart chunk 553 KB —— 同上 |

### 3.4 算法子系统（AMAS）

| 算法 | 当前桶分布近似 P95 | 目标 | 置信 | 依据 |
|---|---|---|---|---|
| heuristic / ige / swd / ensemble | 桶 0–1（≤ 500 µs） | < 1 ms | M | 纯内存计算，无 IO |
| mdm（记忆模型） | 桶 1–2（500-1000 µs） | < 2 ms | M | 含 forgetting curve / DHP 19 个 w 参数计算 |
| mastery | 桶 0–1 | < 1 ms | M | 阈值比较 |

整条 `/api/amas/process-event` 端到端含锁竞争（per-user `std::sync::Mutex`，commit `134bcfe`），P95 估 5–10 ms。

### 3.5 可用性 / 错误率（全站汇总）

| 维度 | 目标 | 置信 | 依据 |
|---|---|---|---|
| 5xx 错误率 | < 0.1% / 滚动 1h | L | 当前无统计，需先加 prometheus exporter |
| 4xx（非 401/429） | < 1% | L | 同上 |
| 月度可用性 | 99.5%（约 3.6 h/月 down） | M | 单实例无 HA；自更新 fork-exec 重启 ~3 s 端口空窗（commit `10d12aa` 修了端口重试） |
| 自更新失败率 | < 5% | M | sha256 校验 + 文件锁 + rollback；最薄弱环节是 GitHub CDN 抖动 |

---

## 4. 容量上限假设（推算 + 待压测验证）

> 单实例 / 1 ECS / SQLite 单库前提。

### 4.1 SQLite 单库

- **库大小**：WAL + mmap 256 MiB；SQLite 单库官方上限 281 TB；**实务上限约 100 GB 后 VACUUM 痛苦**。当前 45 张表（`store/schema.rs`），按假设 1 万活跃用户 × 500 学习记录/月 × 1 KB/记录 ≈ 5 GB/年。
- **WAL checkpoint**：默认 PASSIVE，1000 页阈值；自更新前用 `VACUUM INTO`（`docs/auto-update.md:99`）。
- **并发**：WAL 模式下 1 writer + N readers，pool 16 个连接对应 1 个 writer + 15 个 reader 上限。  
  **写瓶颈推算**：每写事务 1–5 ms（mmap + WAL），单 writer 串行 → **理论上限 ≈ 200–1000 写/s**，实际受 fsync(NORMAL) 限制约 **300 写/s**。
- **读瓶颈**：15 readers × cache 64 MiB × mmap 256 MiB → 读单笔 < 1 ms，**理论 ≈ 10k+ 读/s**。

### 4.2 用户 / 设备

- `system_settings.max_users` 默认 `DEFAULT_MAX_USERS`（需到 `src/constants.rs` 看具体数）
- 单实例 daily active 推算上限：**1k–5k**（写瓶颈先到）；超过需拆 Postgres / shard。
- SSE：1000 并发上限 = 假设场景 `1k DAU × 30% 同时在线 ≈ 300 SSE` 在容量内。

### 4.3 QPS（HTTP）

- rate_limit map 上限 100k IP × 16 shard ≈ 6.25k IP/shard，单 Mutex 抢争用即性能墙。**100k 实际是 DDoS 上限**，正常运行远低于此。
- 单实例 P95 50 ms 假设 + Tokio 多线程 8 cores：**理论吞吐 ≈ 8 × 1000 / 50 = 160 req/s 稳态**，峰值 ≈ 500 req/s（pool 16 起作用）。
- AMAS process-event 写路径：受 SQLite 单 writer 约束 → ≈ 300 events/s。

### 4.4 Cron / 后台

- 21 worker 全开，最高频 5 min × 6 个 = 0.02 op/s 平均；峰值是凌晨 01:00 `daily_aggregation` 单实例独占。**无横向扩缩**。

### 4.5 内存

- SQLite mmap 256 MiB + cache 64 MiB × 16 conn = 1 GiB SQLite 部分；
- Tokio + axum + amas state DashMap ≈ 100–300 MiB；
- 总 RSS 估 **1.5–2 GiB**，需要 ECS 至少 2 GiB 内存（生产 8.135.57.148 已部署，待核实规格）。

---

## 5. 稳定性缺口 TOP 清单

### 5.1 已发生的回归（生产事故）

| # | 时间 | 缺陷 | 根因 | 修复 commit | 教训 |
|---|---|---|---|---|---|
| 1 | 2026-04-21 | Runtime hang，无日志无响应、进程不死 | spawn_blocking 无界 + SQLite pool=4 饱和 | `134bcfe` 加 Semaphore 背压 + std::sync 锁 + pool 4→8 | 后续所有阻塞调用必须走 `run_blocking` |
| 2 | 2026-04-25 | SSE 断连后死锁 | realtime.rs broadcast/tx 释放顺序 | `2b472be` 调整 SSE drop 链 | SSE handler 改动需补 e2e 断连测试 |
| 3 | 2026-05-14 | 高并发 SSE/worker/batch 抢连接 | pool 8 不够 | `2b80575` 4→16 + cache_size/mmap_size/temp_store PRAGMA + semaphore 同步 | pool 与 semaphore 必须同步调；`.env.example` 仍写 4 ⚠️ |
| 4 | 2026-05-14 | apply 长跑超 reqwest 30s total | 单 client 共用 timeout | `075844c` 拆 download_client + read_timeout 60s | 任何 admin 长流程 / 自更新设计，参考 `feedback_admin_self_update_pitfalls` 4 坑点 |
| 5 | 2026-05-15 | systemd Restart=on-failure 对 exit(0) 不重启 | 自更新 fork-exec 后父退 0 | 改 service 为 always | 同上 |
| 6 | 2026-05-18 | `/admin/feedback` 进入即崩 ErrorBoundary | 前端类型签名 items vs data | hotfix `5d49e2f` v0.6.0-beta.2 | `feedback_paginated_field_name_check`：admin API 一律 `data: T[]` |
| 7 | 2026-05-19 | `/api/favorites` 分页空响应 | 未走 paginated() | `fb93944` P3#5 | 客户端契约第三轮审计才捞到 |
| 8 | 2026-05-19 | WordState wire 大小写不一致 | 后端 PascalCase / 客户端 lowercase | `d0325f8` P3#7（breaking） | 同上 |
| 9 | 2026-04 起 | CI flaky | pool_connection_timeout 250 ms 不够 + jsdom matchMedia + vitest IPC race | `8a99df3` 250→2000ms / `ea2ee6b` singleFork / `6973bbd` threads→forks | CI 慢盘环境与 prod 行为不一致，参数应分环境 |

### 5.2 潜在风险点（v1 必须封）

| # | 风险 | 触发条件 | 影响 | 建议处置 |
|---|---|---|---|---|
| P1 | `.env.example` SQLITE_POOL_SIZE=4，代码默认 16 | 运维 copy example | pool 卡到 4，并发塌缩 | `.env.example` 同步 16，发布前自检（[[feedback_release_pre_flight_checks]]） |
| P2 | 无路由级 HTTP latency / 5xx 监控 | 任何性能回归 | 用户先报障，团队后知 | v1 前接入 prometheus exporter 或至少 tower-http `Histogram` |
| P3 | 单实例 + SQLite 单库，无 HA | 自更新 fork-exec 期间 / 进程崩溃 | ≈ 3 s 端口空窗 + 数据库读写都断 | systemd RestartSec=1s 已配；HA 留 v1.1 |
| P4 | SSE 上限 1000 + heartbeat 5 s | 突发流量 / 10k DAU | 第 1001 连接 429，已断用户重连风暴 | exponential backoff（客户端侧），上限提至 5000 需评估文件描述符 |
| P5 | rate_limit 按 IP，不区分用户 | 共享出口（学校 / 公司 NAT） | 多人互相拖累 → 误拦 | v1 拆"未登录按 IP / 登录后按 user_id" |
| P6 | 自更新国内→GitHub 22 KB/s | 无镜像或 ghproxy 挂了 | apply 阶段卡 read_timeout 60s 循环 7 min+ | ghproxy.net 已是默认（[[wordforge_v0_5_release_2026_05_19]]）；准备备用镜像 |
| P7 | reqwest **download_client 无 total timeout** | GitHub CDN 死链 / 网络断 | apply 阶段悬挂直到 60 s read_timeout 触发 | 现有 read_timeout 已防死挂；监控加 phase=downloading > 5 min 告警 |
| P8 | strict-mode 路由豁免列表静态 | 加新 admin / 公共路径忘记加豁免 | 全部 admin / 浏览器直访请求 400 MISSING_OS | 集成测试 `tests/strict_mode_http.rs` 已覆盖核心路径；新路由需追测 |
| P9 | SQLite VACUUM INTO 备份 100% 锁库 | 库 > 5 GB 后 VACUUM 耗时分钟级 | 自更新前 SSE 断、写入阻塞 | v1 加渐进备份 / 增量 / 备份打到 read-only snapshot |
| P10 | `monitoring_aggregate` worker 是 WIP（enabled=false） | 长期不开 | engine_monitoring_events 表只写不聚合，越长越慢 | v1 前必须实现并打开，或加 retention 删旧 |
| P11 | metrics 6 桶 fixed 精度过低 | 真实 P99 > 10 ms 时跑出桶 | 算法回归无报警 | v1 换 prometheus `histogram_quantile` 或 HDR 桶 |
| P12 | 无 backpressure on telemetry payload | 客户端高频 telemetry 灌 | DB 写 + monitoring sample 5% 链路被打爆 | 加 per-user 限频 + payload size guard |
| P13 | AMAS per-user std::sync::Mutex | 同一 user 多设备并发 | 串行执行 + 长事务时阻塞 Tokio worker thread | 已包在 `run_blocking` 里，相对安全；但 amas process-event > 50 ms 时仍可能拖累 |
| P14 | `cargo test --lib` 224 passed、`vitest 859/860`（[[wordforge_client_backend_alignment_2026_05_19_v3]]） | 1 个跳过测试在 admin 流程 | 可能含未捕获回归 | v1 GA 前归零 skip |
| P15 | release `prerelease` 规则刚加（`release.yml` contains '-'） | 误打 v1.0.0-rc1 标签未带破折号 | 升级器把 rc 视为 stable 推给用户 | v0.6.0-beta.3 已修，[[wordforge_v0_6_0_beta_3_release]] |

---

## 6. v1 必做的性能 / 容量改造项

> 优先级：P0 必做、P1 强烈建议、P2 留观察。

### 6.1 P0（拍板前必须落地）

1. **路由级延迟监控**：tower-http 加 `MakeHistogram`（自实现）或集成 `axum-prometheus`，导出 `http_request_duration_seconds{route, method, status}`，开 `/metrics` 端点（admin 鉴权）。
2. **`.env.example` 与代码默认对齐**：`SQLITE_POOL_SIZE=16`，避免运维踩坑。
3. **monitoring_aggregate worker 实装或下架**：`engine_monitoring_events` 表必须有 retention（建议 30 天滚动删除）。
4. **5xx 错误率告警**：基于上一项 metric，新增 `error_rate_5min > 1%` 触发 admin SSE 告警。
5. **自更新 phase 超时**：apply 阶段任何 phase（downloading / verifying / extracting）> 5 min 主动 abort + rollback，避免悬挂。

### 6.2 P1（v1 GA 强烈建议）

1. **保持 SQLite 单库**：v1 用户量级（< 5k DAU）SQLite 够用，**不要换 Postgres**——增运维复杂度、丢失单二进制部署优势；留 v2 视真实用户规模再切。
2. **加 prometheus exporter**：替换 / 补强当前 AlgorithmMetrics 桶（保留作 anomaly trigger）。
3. **rate_limit 区分匿名 / 已登录**：登录后按 user_id，避开 NAT 互拖。
4. **SSE 上限提至 5000 + 心跳改 10 s**：减轻 watchdog 扫描压力，文件描述符配 `ulimit -n 65535`。
5. **前端首屏拆 vendor-echarts / vendor-codemirror**：当前 EChart 553 KB / Probe 414 KB 是首次进 admin 的延迟来源；可考虑 admin 子应用独立 chunk + service-worker preload。
6. **batch API 与 N+1 完整审计**：commit `e616047` 修了 VocabularyPage 一处，但 admin / clients 列表未审。
7. **自更新备份**：库 > 1 GB 时 `VACUUM INTO` 耗时长，加进度上报；准备 read-only snapshot 路径。
8. **client SDK 重试规范**：429 / 5xx 客户端 exponential backoff，避免雪崩。

### 6.3 P2（留观察 / v1.1）

1. PGO：编译期 profile-guided，对热点算法路径估 < 5%，ROI 低。
2. SQLite → libSQL / Turso（带逻辑复制）：v1.1 探索。
3. CDN：static/ 整体打 7 MiB 不大，CDN 收益有限；可考虑国内 OSS + 自更新走相同源。
4. HA：单实例 → 2 实例 + 共享 SQLite（不可行 WAL 不跨进程）→ Postgres 切换。

---

## 7. v1 GA SLA 数字提案（带置信度）

> 适用环境：单实例阿里云 ECS（≥ 2 vCPU / 4 GiB RAM）、用户规模 1k–5k DAU。  
> 置信度全部 L–M（无生产实测数据）。  
> **GA 前必须用 k6 / wrk 压一次**：登录 / 学习会话 / 复习提交 / favorites 列表 / SSE 建连，5 个核心路径各 10k 请求验证。

### 7.1 用户路径 SLA

| 指标 | 目标 | 置信 |
|---|---|---|
| `/api/auth/login` P95 | < 300 ms | M |
| `/api/learning/sessions` POST P95 | < 250 ms | L |
| `/api/learning/sessions/:id/complete` P95 | < 300 ms | L |
| `/api/records`（单条） P95 | < 150 ms | L |
| `/api/words/batch-get` P95 | < 120 ms | M |
| `/api/favorites?page` P95 | < 100 ms | M |
| `/api/realtime/events` 建连 P95 | < 500 ms | M |
| 前端首屏 LCP（中端 Android） | < 2.5 s | L |
| 前端路由切换 TTI | < 1 s | L |

### 7.2 全站 SLA

| 指标 | 目标 | 置信 |
|---|---|---|
| 月度可用性 | 99.5%（≤ 3.6 h down/月） | M |
| 5xx 错误率（不含限流 429） | < 0.1% / 滚动 1h | L |
| 4xx 业务错误率 | < 2% | L |
| 自更新成功率 | > 95% / 季度 | M |
| 自更新 apply（不含下载） | < 90 s P95 | M |
| SQLite 库大小 | < 5 GiB | M |
| 单实例稳态 QPS | ≥ 100 req/s | L |
| 单实例峰值 QPS | ≥ 300 req/s | L |
| SSE 并发 | ≥ 1000 active | M |

### 7.3 算法 SLA

| 指标 | 目标 | 置信 |
|---|---|---|
| AMAS prediction_composite | ≥ 1.10 vs DEFAULT_MEMORY_MODEL_CONFIG | H（已实测） |
| AMAS DHP expectedMemory | ≥ 3000 | H（已实测 3154） |
| ICI（校准） | < 0.05 | H（已实测 0.0379） |
| `/api/amas/process-event` P95 | < 50 ms | M |

### 7.4 已知折让

- DHP `targetCount` 当前比 baseline 低 13.4%（`docs/amas-tuning-2026-05-15/01-final-report.md:57`），写入 v1 GA "已知限制"，不阻塞发布。
- 算法 split=test 未跑泛化验证（同上 §9.2），GA 前应补一次。

---

## 8. 文件 / commit 索引

**代码锚点**：
- `src/config.rs:280-470`（Config::from_env，所有限额）
- `src/store/mod.rs:46-83`（SQLite + r2d2 配置）
- `src/blocking.rs:35-77`（全局 Semaphore + run_blocking）
- `src/middleware/rate_limit.rs:17-141`（16 shard rate limiter）
- `src/middleware/strict_mode.rs:27-`（客户端契约）
- `src/routes/realtime.rs:54-180`（SSE handler + 心跳）
- `src/services/updater.rs:175-247`（updater + 双 reqwest client）
- `src/workers/mod.rs:156-263`（21 job cron）
- `src/amas/metrics.rs:8-78`（算法运行时指标）
- `src/routes/mod.rs:53-129`（中间件栈装配）
- `Cargo.toml:83-86`（release profile）
- `frontend/vite.config.ts`（manualChunks）

**关键 commit**（按时间倒序）：
- `2b80575` 2026-05-14 perf(sqlite): pool 4→16 + cache_size/mmap_size/temp_store
- `075844c` 2026-05-14 fix(updater): mirror prefix + download client read_timeout
- `134bcfe` 2026-04-21 fix: bounded blocking pool + std::sync locks（修运行时挂死）
- `2b472be` 2026-04-25 fix: prevent SSE disconnect deadlock
- `8a99df3` 2026-04 fix(store): pool_connection_timeout 250→2000ms（CI flaky）
- `e616047` 2026-02-15 全项目速度优化：压缩 / 缓存 / 索引 / 批量 API / 构建

**已有研究产物**：
- `docs/amas-tuning-2026-05-15/01-final-report.md`（算法基线，本文 §1.1 / §3.4 / §7.3 引用）
- `docs/v1-research/01-final-report.md`（FSRS/DHP 全景调研，arch-scout 输出）
- `docs/v1-research/02-fsrs-dhp-research.md`
- `docs/v1-research/03-adapter-analysis.md`
- `~/.wordforge-bench/maimemo/reports/`（benchmark 持久化结果）

**记忆库相关条目**：
- `[[amas_tuning_results_2026_05_15]]`
- `[[feedback_admin_self_update_pitfalls]]`
- `[[feedback_release_pre_flight_checks]]`
- `[[feedback_paginated_field_name_check]]`
- `[[wordforge_v0_5_release_2026_05_19]]`
- `[[wordforge_v0_6_0_beta_3_release]]`
- `[[wordforge_prod_deployment]]`

---

## 9. 结论与给 v1 RFC 的输入

**可以拍板的（H/M 置信）**：
1. 算法基线已实测：prediction +10.6% / memory +14% / ICI -25%，GA 直接用 nearMiss[0] 配置。
2. SQLite + WAL + pool 16 + 64 MiB cache + 256 MiB mmap 的容量假设：**1k–5k DAU、< 5 GiB 库、稳态 100 req/s、峰值 300 req/s** 不需要换库。
3. 单实例 + systemd 重启 + fork-exec 自更新 + ghproxy 镜像的发布链路已稳（v0.5.0–v0.6.0-beta.3 七连发零干预）。
4. v1 不引入 Postgres / 不拆库 / 不上 HA；用监控 + 限流 + SLO 卡控来保 GA。

**必须先压测才能定的（L 置信）**：
1. 所有 HTTP 路由 P95 / P99 提案值 —— GA 前 k6 跑核心 5 条路径，目标见 §7.1。
2. 前端首屏 LCP / TTI —— GA 前 Lighthouse + Web Vitals 实测。
3. SSE 1000 上限的真实文件描述符压力。

**最大单点风险（按风险×可能性排序）**：
1. **无路由级监控**（§5.2 P2 / §6.1 P0#1）—— 任何回归用户先报障。
2. **`.env.example` 与代码默认不一致**（§5.2 P1）—— 部署一次就踩。
3. **monitoring_aggregate WIP**（§5.2 P10）—— `engine_monitoring_events` 表只写不删。
4. **`download_client` 无 total timeout**（§5.2 P7）—— 自更新最长可能挂数十分钟。

**给 release-keeper 的对接点**：
- v1 GA 必须新增的运维 SOP：每月 SQLite 库大小巡检、k6 压测脚本入仓、5xx 告警 runbook、自更新失败回滚 runbook（部分已在 `docs/auto-update.md:102`）。
- `.env.example` ↔ 代码 default 同步纳入发版前 self-check（[[feedback_release_pre_flight_checks]] 加第 4 条）。

**给 arch-scout 的对接点**：
- v1 不切 Postgres；架构图保持单二进制 + SQLite + r2d2 pool 16。
- AMAS engine 锁层级（per-user std::sync::Mutex + run_blocking）保持；不要引入 async 锁。

**给 signal-miner 的对接点**：
- 用户最敏感的延迟在"复习提交后下一题出现"——对应 `/api/records` + `/api/learning/sessions/...` 链路 P95 < 300 ms，应在用户体验调研里单独问。
