# wordforge-loadtest

WordForge 学习端 `/api/*` 并发承载压测工具。目标：测出 dev 那台 **2h2g** 服务器能扛多少**同时学习**的设备，并观察后端性能优化效果。

独立 crate（自带空 `[workspace]`，不影响仓库根 `learning-backend` 的 `cargo build`）。

## 设计要点

- **忠实复刻客户端学习时序**：`create session → next-words 取批 → 逐词(pick-next-word → generate-options → 答题) → 周期 sync-progress → complete-session`，压到后端真正吃 CPU/DB/AMAS 的路径（`next-words` 选词、`sync-progress` 事件摄入 + 记忆模型更新最重）。
- **认证与稳态负载解耦**：先 `provision` 预注册账号池缓存 token（access token 默认 24h），`run` 阶段只用 token，**不碰注册/登录端点**——否则注册端点 `10/60s/IP` 限流会成为爬坡假瓶颈。
- **交互式加减设备**：运行中输入数字设并发、`+N`/`-N` 增减、`q` 退出。缩容用 CAS 保证不过冲。
- **生成器自身低开销**：tokio 异步 + 无锁原子直方图，几千并发不会假性成为瓶颈。

## 两种测试目标

| 模式 | 客户端行为 | 服务器侧限流 | 测的是 |
|---|---|---|---|
| `--mode raw` | 闭环最大吞吐，无 think-time | **放开**（见下） | 后端**裸容量**极限 |
| `--mode realistic` | 每词注入人类 think-time(1.2~3.5s) | **保留默认** | 限流下**真实承载**人数 |

### 裸容量模式：dev 服务器需放开限流（纯 env，无需改代码）

后端限流全是 env 驱动（`src/config.rs`）。在 dev 启动 `learning-backend` 前设：

```bash
export RATE_LIMIT_AUTHENTICATED_MAX=100000000
export RATE_LIMIT_ANONYMOUS_MAX=100000000
export AUTH_RATE_LIMIT_MAX=100000000
# STRICT_MODE_ENABLED 默认 false，无需动
```

> realistic 模式不要设这些，保留默认（认证用户 600/900s≈40/min，注册 10/60s）才测得到"限流下能同时学多少人"。

## 用法

### 1. 预注册账号池（一次性）

```bash
# 建议在放开 AUTH_RATE_LIMIT_MAX 的前提下跑，否则会被 10/60s 卡很久
cargo run --release -- provision --url http://<DEV_IP>:3000 --count 500
# -> 生成 accounts.json（500 个账号 + token）
```

账号池大小 = 你打算压到的**最大并发设备数**。要测到 500 并发就 provision ≥500 个。
`provision` 幂等：邮箱已存在自动转登录，可重跑。

### 2. 跑压测

```bash
# 裸容量：从 50 起，自动每秒爬 20，直到账号池上限
cargo run --release -- run --url http://<DEV_IP>:3000 \
    --mode raw --start 50 --ramp 20 --csv raw.csv

# 真实承载：从 20 起，纯手动加减观察拐点
cargo run --release -- run --url http://<DEV_IP>:3000 \
    --mode realistic --start 20 --csv real.csv
```

运行中交互：

```
100   <- 设并发为 100
+50   <- 再加 50
-30   <- 减 30
q     <- 退出
```

账号文件不存在时可加 `--provision N` 自动先注册。

## 仪表盘读法

```
并发设备:   120 / 目标   120 / 池上限   500
RPS:      480   总请求   85213   错误    12 (0.01%)   限流 0
延迟(全端点)  p50    38ms   p95    210ms   p99    880ms
endpoint          reqs    errs    p50    p95    p99
session          10231       0    9ms   25ms   60ms
next-words        9980       3   45ms  180ms  520ms
sync-prog        15044       5   60ms  240ms  900ms
...
错误码: TIMEOUT=8  HTTP_500=4
```

**找拐点的判据**（任一触发即视为接近 2h2g 上限）：
- p95/p99 延迟随并发陡升（肘部）；
- 错误率 > 1%，或出现 `TIMEOUT`/`HTTP_5xx`；
- RPS 不再随并发增长反而回落；
- realistic 模式下 `RATE_LIMITED` 增多 = 单用户已逼近 40/min 配额。

`--csv` 产物可直接画"并发 vs RPS / p95 / 错误率"曲线定位容量。

## 前置条件

- dev 服务器 DB 里要有**词库内容**，否则 `next-words` 返回空批，工具会直接收尾会话（仪表盘上 `next-words` 正常但 `gen-options`/`sync` 偏少）。
- 生成器默认跑在你本机指向 `--url`；想更贴近真实网络可把二进制 `scp` 到另一台机器跑。
- 字段契约对齐后端 `src/routes/learning/*` 与 `src/routes/auth.rs`（全 camelCase）。
