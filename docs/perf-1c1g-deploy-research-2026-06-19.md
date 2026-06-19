# 1核1G 部署最大化带机量 — 深度调研报告

> 日期：2026-06-19
> 目标：在 1 vCPU / 1 GiB RAM 最小配置服务器上，最大化「综合端到端带机量」（同时在线 SSE 长连接 + 峰值 QPS + 总 DAU 三者整体最大化）；**1 GiB 内存是首要硬约束**。
> 方法：5 角度 fan-out 网络检索 → 23 源 → 111 断言 → 25 条 3 票对抗式验证（19 确认 / 6 证伪）→ 对齐本仓真实代码现状。
> 结论一句话：**研究确认你们 2026-05 的工程决策方向全对，DB 层 6 大结论已落地 5 个**。1C1G 比团队原假设的 2C4G 更紧，本报告只给「针对 1C1G 的增量调整」，不重复已做项。

---

## 0. 最重要的判断

1. **不要换数据库**。研究与你们 perf-warden §6.2 P1#1 结论一致：1C1G + 读重 + 单机 + 重运维简洁性场景，SQLite+WAL 是**正确选择**；上 Postgres 仅在「需多主机部署 / 流复制 / 高并发写」三种硬触发线时才值得，而这三条 1C1G 都不成立。Postgres 自身常驻 baseline 就吃掉 1 GiB 的 10%+，在 1C1G 上是净负。Redis 同理（多一个进程吃内存，应用内 DashMap 缓存已够）。

2. **1C1G 下「带机量」是被三条线同时夹的 min 函数**：
   `带机量 ≈ min(内存上限, CPU 上限, SQLite 写锁上限, FD 上限)`
   - **内存**：决定能"挂"多少长连接 + 多大 page cache（你们已识别为主约束）。
   - **CPU（单核）**：决定 goodput（req/s）——你们 2026-06-01 压测已实测 CPU 是 goodput 瓶颈、~300 req/s 突发打爆 TIME-WAIT。
   - **写锁**：SQLite 单 writer 串行，实测约 300 写/s（perf-warden §4.1）。
   - **FD**：每条 SSE = 1 fd，systemd 默认上限会封顶长连接数。
   "综合容量"最大化 = 同时把这四条线各自抬高，不能只抠一条。

3. **你们已经做对了绝大部分**（见 §1 审计表）。针对 1C1G 真正值得动的增量只有 6 项（§2），其中**3 项零代码改动**（env / systemd / swap），**3 项需改代码且都应"先实测再上"**。

---

## 1. 研究结论 ↔ 本仓现状审计

> 这是报告的核心：每条研究确认的结论，对到你们的实际代码，判定「已做 / 待调 / 缺口」。

| # | 研究确认结论（置信度） | 你们现状（代码锚点） | 判定 |
|---|---|---|---|
| 1 | SQLite+WAL 是单机正确选择；单 writer 是根本并发瓶颈（high） | `journal_mode=WAL`（`store/mod.rs:76`） | ✅ 已做 |
| 2 | 同步 rusqlite 必须经 `spawn_blocking` 卸载，绝不能在 async future 内阻塞（high） | `blocking.rs:run_blocking` + 全局 Semaphore 背压，所有 DB 调用走它（`134bcfe` 修运行时挂死） | ✅ 已做，且比研究建议更进一步（加了背压信号量） |
| 3 | `synchronous=NORMAL` 在 WAL 下完全防腐败、仅 checkpoint fsync（high） | `synchronous=NORMAL`（`store/mod.rs:77`） | ✅ 已做 |
| 4 | **per-connection cache 内存乘积陷阱**：`cache_size × pool` 会爆内存，1GB 上必须调小（high，研究 2-1 分歧点的关键修正） | 代码注释逐字命中此坑：`cache_size=-16000`(16MiB) + pool 16→8，注释写明"压测实测 -64000×16≈1GB 峰值 OOM 风险"（`store/mod.rs:72-73`、`config.rs:342-344`） | ✅ 已踩坑并修正——这是研究专门警示、多数人会错的点 |
| 5 | 读升写的瞬时 `SQLITE_BUSY` 无视 busy_timeout，须用 `BEGIN IMMEDIATE` 预取写锁（high） | `with_user_tx` 用 `TransactionBehavior::Immediate`，注释逐字解释"DEFERRED 读后写经典坑"（`store/mod.rs:223-228`） | ✅ 已做——研究的第二个易错点你们也已修 |
| 6 | PRAGMA 基线 `busy_timeout / mmap_size / temp_store=MEMORY`（high） | `busy_timeout=5000` + `mmap_size=128MiB` + `temp_store=MEMORY`（`store/mod.rs:79-82`） | ✅ 已做 |
| 7 | 高写压下应用层写队列 / 单写连接串行化（high） | **未做**：homogeneous pool（8 连接都可读写）+ 调用侧 per-user `std::sync::Mutex` + IMMEDIATE + busy_timeout | ⚠️ 缺口，但**测试门控**（见 §2.6） |
| 8 | jemalloc `narenas:1` + 缩短 decay 压 RSS、释放给 OS page cache（high） | **部分**：用 glibc + `MALLOC_ARENA_MAX=2`（systemd unit:24，v1.1.3-N4），未上 jemalloc | ⚠️ 可再进一步（见 §2.2） |
| 9 | 长连接带机量本质是内存问题，1GB 真实上限远低于 500K，Rust/axum+SSE 实际低万级（medium） | SSE 上限硬编 1000（`LIMITS_MAX_SSE_CONNECTIONS`，`realtime.rs`） | ⚠️ 1000 对 1C1G 可能已接近上限，需按实测（见 §2.4） |
| 10 | 单实例 k6 足够打满（30K-40K VU/~300K RPS，远超被测）（high） | 你们 §7 已规划 GA 前 k6 压 5 条核心路径，但**脚本未入仓** | ⚠️ 待补（见 §3） |
| 11 | sysctl 抗突发（TIME-WAIT 等） | `tcp_max_tw_buckets=262144`（`sysctl.d/99-wordforge.conf`，基于自己压测）+ nginx keepalive 样例 | ✅ 已做 |

**结论**：DB/运行时层（#1-6）几乎满分，连研究里两个最容易踩的坑（per-conn cache 乘积、读升写死锁）你们都已独立踩过并修好。剩下真正能动的增量集中在**内存分配器、连接池在 1C1G 的取值、SSE/FD 上限、压测落地**。

---

## 2. 针对 1C1G 的增量调整（按 ROI 排序）

### 2.1【零代码·P0】连接池在 1C1G 收到 4–6，用你们自己的 pool 监控实测定值

**为什么**：当前 prod 默认 `SQLITE_POOL_SIZE=8`。1C1G 上每连接 cache ~15.6MiB → 8×≈125MiB 常驻峰值（仅活跃热页时），占 1GB 的 ~12%。**单核上 pool>核数 的并发收益本就有限**（CPU 串行化），但内存代价线性。

**怎么做**（纯 env，blocking 信号量会自动跟随 `init_blocking_semaphore(pool_size)`，`main.rs:218`）：
```bash
# .env（1C1G 起步值，先 6 后视监控降到 4）
SQLITE_POOL_SIZE=6
SQLITE_CONNECTION_TIMEOUT_MS=1000   # 池变小后给突发留排队余量，避免 250ms 硬失败
```
- 省内存：8→6 省 ~31MiB，8→4 省 ~62MiB（直接转为 OS page cache，惠及所有 mmap 读）。
- **风险与护栏**：2026-05-14 事故#3 是"pool 8 不够"被 SSE/worker/batch 抢连接。但你们 admin 已内建 `pool_status()`（`store/mod.rs:175`，m023 资源条）——**上线后直接看 connections/idle 与 connection_timeout 错误率**：若频繁打满或超时多，再回调。这是可观测、可回退的调参，不是一次性赌注。
- ⚠️ **不要照搬 perf-warden §6.2 P1#4 的"SSE 上限提 5000"**——那是 2C4G 假设下的建议，1C1G 不适用。

### 2.2【需改代码·P1，先实测】jemalloc + narenas:1 取代 glibc，把内存主动还给 OS

**为什么值得在已有 `MALLOC_ARENA_MAX=2` 之上再做**：glibc malloc 即使限了 arena 数，**几乎不把 free 的内存归还给 OS**（留在 arena free-list），RSS 会爬到峰值后**不降**。jemalloc 配 `dirty_decay_ms/muzzy_decay_ms` 会**主动 purge 脏页还给内核**——在 1GB 这种没有余量的机器上，"峰值后能回落"比"峰值低一点"更关键。研究证据：Meilisearch 实测降 RSS→释放更多 page cache→减磁盘读→提升性能（对依赖 OS page cache 的 SQLite 尤其适用，无 InnoDB 那种双缓冲浪费）；jemalloc 官方 TUNING.md 逐字把 `narenas:1` 列为"低资源应用"推荐。

**容器 CPU 误检坑（研究确认）**：1-vCPU 云 VM 常被 jemalloc 误检为宿主核数 → 默认开 4×host_cores 个 arena（16-64+），pin `narenas:1` 有真实 RSS 节省（DuckDB PR#16046 / GitLab MR#2506 佐证）。

**怎么做**：
```toml
# Cargo.toml
[target.'cfg(not(target_env = "msvc"))'.dependencies]
tikv-jemallocator = "0.6"
```
```rust
// main.rs 顶部
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// 关键：decay 必须配 background_thread:true 才会真正驱动 purge（研究 caveat：jemalloc issue #2688/#2751）
#[cfg(not(target_env = "msvc"))]
#[allow(non_upper_case_globals)]
#[export_name = "malloc_conf"]
pub static malloc_conf: &[u8] = b"narenas:1,background_thread:true,dirty_decay_ms:5000,muzzy_decay_ms:5000\0";
```
上线后保留 `MALLOC_ARENA_MAX=2` 无害（jemalloc 接管后该 env 对 glibc 失效）。
- **诚实定级 P1 而非 P0**：你们 v1.1.3-N4 已把 arena 碎片定性为内存**次因**（主因 SQLite pcache 已由 cache_size 收口）。所以这是"锦上添花 + 1GB 无余量场景的保险"，**必须用 `/api/admin/health/metrics` + RSS 监控做上线前后对比**再决定保留与否，别当主推。预期：RSS 峰值后回落更干净，稳态低 10-30%（需实测验证）。
- 备选：`mimalloc` crate（更简单、也擅长低 RSS），二选一即可。

### 2.3【零代码·P0】systemd 显式设 `LimitNOFILE` —— 当前 unit 缺这一项（真缺口）

**为什么**：`deploy/wordforge.service.tmpl` **没有 `LimitNOFILE`**（已核对全文）。每条 SSE 长连接 = 1 fd，加 DB/WAL/socket/日志，systemd 服务默认 nofile 在部分发行版只有 1024 soft，会**直接封顶长连接带机量**。研究确认 FD 上限确需抬（但证伪了"必须抬到 1M+"的说法——按实际目标连接数 + 余量设即可，盲目设 1M 浪费内核内存）。

**怎么做**（在 unit `[Service]` 段加）：
```ini
LimitNOFILE=65536
```
配合 SSE 上限 1000 + HTTP socket + DB，65536 是宽松且不浪费的值（不要照搬网上的 1048576）。

### 2.4【需改代码/配置·P1】SSE 上限 1000 按 1C1G 实测重定，别盲目抬高

研究（finding 9，medium，secondary 源）：1GB 长连接理论上界 70K-350K 是**空连接**乐观值；真实 Rust/axum+SSE **计入进程 RSS + SSE channel buffer + 活跃消息后实际低万级**，且 1C1G 比 source 假设的整机更紧。

**做法**：先实测**单条空闲 SSE 连接的真实内存**（tokio task + broadcast channel buffer + tower 中间件栈），再据 `可用内存 ÷ 单连接内存` 反推安全上限。在拿到实测数前，**保持 1000 不动**（1000 对应你们假设的 1k DAU×30% 在线≈300，仍有余量）。这是研究 openQuestions 里点名"需对本项目实测"的项。

### 2.5【需改代码·P1】单核下精简 brotli 动态压缩，换 goodput

**为什么**：中间件栈含 `CompressionLayer`（gzip+br，`main.rs:516`、`routes/mod.rs`）。**brotli 对动态响应是 CPU 重活**，单核上 goodput 瓶颈正是 CPU（你们已实测）。每个动态 JSON 响应跑 br 高质量压缩，挤占的就是 req/s。
**做法**（任选）：动态响应只留 gzip（br 留给 ServeDir 静态资源，静态可预压缩一次）；或把 br 质量降到 4-5。tower-http `CompressionLayer` 可按 content-type/predicate 区分。预期：动态路径 CPU 下降 → 峰值 QPS 上升（需压测量化）。

### 2.6【架构·P2，测试门控】写队列 / 单写连接 —— 仅当压测见 BUSY 才做

研究 finding 7 建议高写压下用**单写连接 + mpsc 队列**串行化所有写（bugsink/tenthousandmeters：256 并发线程朴素写产生 SQLITE_BUSY，队列串行稳定 ~57k ops/s）。你们当前是 pool-of-8 + per-user `std::sync::Mutex` + IMMEDIATE + busy_timeout，**这是标准"够用"模式**。
- **现在不要做**：你们压测瓶颈是 CPU 不是写锁，且写队列会把"所有用户的写"串到一条线，对多用户突发写未必更快（SQLite 写锁本就全局，队列只是把等待从 busy-retry 挪到队列里——更干净、消除 BUSY 重试风暴，但吞吐是平的）。
- **触发条件**：上线压测若出现 `SQLITE_BUSY` 错误率上升 / 写路径 p99 尾延迟尖刺，再上"1 个专用写连接 + N 个只读连接"拆分（同时还能压低"连接数×cache_size"内存乘积——只读连接也可少给 cache）。

---

## 3. 压测与容量规划（落地 perf-warden §6.1 P0#1 的最大单点风险）

你们自己把"无路由级监控"列为头号风险。研究确认 **k6 单实例足够**打满 1C1G（被测系统能力远低于 k6 的 30K-40K VU 上限），且 **k6 必须跑在独立机器**（勿与被测同机，1000 VU 吃 1-5GB 会污染测量）。

**1C1G 专项压测方案**：
1. 压 5 条核心路径（你们 §7 已定）：login / learning.sessions / records 单条 / favorites 分页 / SSE 建连。
2. 同时盯 5 个指标定位瓶颈落在哪条线：
   - **RSS**（`/api/admin/health/metrics` + `pool_status`）→ 内存线
   - **CPU**（单核 %，看是否 100% 饱和）→ goodput 线
   - **p99 延迟**分路径 → 哪条路径先塌
   - **SQLite 写锁等待 / BUSY 计数** → 写锁线（决定是否要 §2.6 写队列）
   - **连接数 / fd 数**（`ss -s`）→ FD 线（决定 §2.3 LimitNOFILE）
3. 容量推算：固定并发阶梯加压，记录每条线先到的拐点 = 该机带机量上界。哪条线先到就先优化哪条，循环。

---

## 4. 被对抗式验证**证伪**的"坊间说法"（避坑，别照抄网文）

研究对 25 条断言做 3 票对抗验证，以下 6 条被判**不成立**（已从结论剔除），网上常见但别信：

| 证伪断言 | 票数 | 真相 |
|---|---|---|
| 抬高 busy_timeout 指数级降低锁失败 | 0-3 | 读升写死锁会**无视 timeout 立即 BUSY**，纯抬 timeout 扛不住高并发写，须配 IMMEDIATE/写队列 |
| 1000 并发写者高 timeout 下 p50=2.3s 可扛 | 0-3 | 极端容错数字不成立，别拿 SQLite 硬扛 1000 写者 |
| SQLite 单机吞吐/尾延迟优于 stock Postgres（5.1 vs 4.1 req/s 等具体数） | 0-3 | 具体对比数字不可靠，只保留"读重单机选 SQLite"的**定性**判据 |
| Linux 默认 1024 FD 应抬到 1M+ | 0-3 | FD 确需抬，但"1M+"是浪费；按目标连接数+余量（本报告 65536）即可 |
| idle 2-10KB/active 10-100KB 作容量模型**关键输入** | 1-2 | 单连接内存因实现而异，**必须对本应用实测**，别拿网文数字当模型输入 |
| SQLite 适用上限是 ~1000 写/s 这条硬线 | 0-3 | 无统一阈值，按你们实测（~300 写/s @ NORMAL fsync）为准 |

---

## 5. 1C1G 部署清单（可直接执行）

**零代码（env / 系统 / 部署）**：
- [ ] `SQLITE_POOL_SIZE=6`（起步，按 `pool_status` 监控降到 4 或回 8）
- [ ] `SQLITE_CONNECTION_TIMEOUT_MS=1000`（池变小后吸收突发）
- [ ] systemd unit 加 `LimitNOFILE=65536`（**当前缺**，§2.3）
- [ ] 确认 `MALLOC_ARENA_MAX=2` 已生效（已在 unit:24）
- [ ] 确认 `sysctl.d/99-wordforge.conf` 已 `sysctl --system` 部署（含 `tcp_max_tw_buckets`）
- [ ] 加 1-2 GiB **swap 文件 + `vm.swappiness=10`**：1GB 无余量，swap 是**防 OOM-kill 的安全网**（VACUUM / 凌晨 daily_aggregation / batch 写的瞬时尖峰），不是容量——别让它常态化换页（那会毁延迟）
- [ ] `.env.example` 的 `SQLITE_POOL_SIZE` 与代码默认对齐（perf-warden §5.2 P1 老缺口，发布前自检）
- [ ] 反向代理：若已在 CDN/LB 后则跳过；自管 TLS 用 nginx（你们已有 keepalive 样例）做 **TLS 卸载**（单核 RSA 握手贵，卸载省的 CPU 对 goodput 是净正）+ 连接缓冲，但配 `worker_processes 1` 压 nginx 自身 RSS（~10-20MiB）

**需改代码（都先实测再上）**：
- [ ] jemalloc + `narenas:1,background_thread:true,decay` （§2.2，P1，RSS 前后对比验证）
- [ ] 动态响应精简 brotli → gzip-only 或降质（§2.5，P1，压测量化 QPS 增益）
- [ ] SSE 上限按单连接实测内存重定（§2.4，P1）
- [ ] （仅压测见 BUSY 才做）单写连接 + 写队列（§2.6，P2）

**必做但属团队既有 backlog（非本研究新增）**：
- [ ] 路由级 HTTP latency / 5xx 监控（perf-warden §6.1 P0#1，头号风险）
- [ ] k6 压测脚本入仓（§3）

---

## 6. 开放问题（研究未能定量，须对本项目实测）

1. 单核 tokio `worker_threads` 最优取值（默认=1 vs 显式 2）与 `max_blocking_threads`/pool 精确配比——研究无定论，按实测。默认（=核数=1）+ 信号量背压在单核上是合理起点。
2. 反向代理在 1GB 下净收益正负（nginx 自身 RSS vs TLS 卸载省的 CPU/内存）——研究列为开放问题，建议按"是否已有 CDN/LB"二分决策。
3. 单条空闲 SSE 连接的实测内存（校准 §2.4 的上限）。
4. 内核 sysctl（somaxconn / tcp_max_syn_backlog / ip_local_port_range / tcp_tw_reuse）在目标连接规模的具体推荐值——研究未逐项取证，你们已有 `tcp_max_tw_buckets` 实测先例，按同法压测补齐。

---

## 7. 主要来源（按角度）

- **SQLite 官方一手**：`sqlite.org/wal.html`、`pragma.html`、`mmap.html`、`rescode.html`、`lang_transaction.html`（WAL 语义 / synchronous=NORMAL 防腐败 / IMMEDIATE 取锁 / BUSY_SNAPSHOT）
- **tokio 官方**：`docs.rs/tokio spawn_blocking`、`tokio.rs/tutorial/shared-state`（阻塞卸载）
- **jemalloc 官方**：`github.com/jemalloc/jemalloc TUNING.md`（narenas:1 / decay）
- **实测博客（已与官方交叉核实）**：phiresky SQLite tuning、oneuptime 2026 SQLite production、berthub（database locked despite timeout）、tenthousandmeters（并发写基准）、bugsink（single-writer 架构）、Meilisearch（RSS→page cache）
- **容量/压测**：Grafana k6 large-tests 官方文档、websocket.org connection-limits（secondary）
- **架构权衡**：intuitem 2026 PG vs SQLite 基准（定性）、daily.dev / twilio / sitepoint SQLite production 2026

> 置信度与时效：DB 层结论 high（官方文档交叉核实）；长连接容量 medium（secondary 源，须实测校准）；jemalloc/k6 数字为 2026 latest 文档。完整 19 条确认断言 + 6 条证伪 + caveats 见工作流原始输出。
