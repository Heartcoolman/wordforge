# 架构概览

## 整体拓扑

```
┌─────────────────────────────────────────────────────────────────┐
│                    learning-backend (单二进制)                   │
│                                                                  │
│   ┌──────────────┐    ┌──────────────────┐   ┌──────────────┐  │
│   │ axum Router  │───▶│   AppState       │──▶│   Store      │  │
│   │  /api/*      │    │  - amas: Arc<…>  │   │ (SQLite WAL) │  │
│   │  /admin/*    │    │  - store: Arc<…> │   │  r2d2 pool   │  │
│   │  /health     │    │  - updater       │   └──────────────┘  │
│   └──────┬───────┘    │  - rate_limit    │                     │
│          │            └────────┬─────────┘   ┌──────────────┐  │
│          ▼                     │             │ AMASEngine   │  │
│   静态文件 (static/)              ├────────────▶│ +16 子配置   │  │
│   ↑ 前端构建产物                 │             │ +热加载       │  │
│                                │             └──────┬───────┘  │
│   ┌───────────────────────┐    ▼                    │          │
│   │  Workers（leader 实例） │────tokio::spawn         │          │
│   │  - config_watcher     │◀────────── notify ──────┘          │
│   │  - llm_advisor        │                                    │
│   │  - update_checker     │                                    │
│   │  - 17 个聚合/清理 job  │                                    │
│   └───────────────────────┘                                    │
└─────────────────────────────────────────────────────────────────┘
```

## 后端骨架（`src/`）

| 模块 | 职责 |
|---|---|
| `main.rs` | 启动入口：加载 env / AMASConfig → 建 Store → 建 AMASEngine → AppState → 起 Router + Workers |
| `config.rs` | 环境变量解析为强类型 `Config`（含 `UpdateCheckConfig`、`LlmConfig`、`AmasConfigOverrides` 等） |
| `state.rs` | `AppState`：注入所有共享依赖给 handler |
| `routes/` | HTTP 路由：`auth` / `users` / `words` / `wordbooks` / `learning` / `admin/*` / `realtime`（SSE） |
| `services/` | 业务编排：`updater`（自更新）、`llm_provider`、`broadcast`、`metrics_reporter` 等 |
| `store/` | SQLite 持久层：`schema.rs`（DDL）/`migrate.rs`（版本化迁移）/`operations/*`（按表分文件） |
| `amas/` | AMAS 核心：见下方"AMAS 模块" |
| `workers/` | 后台任务（共 20 项）：见下方"Workers" |
| `blocking.rs` | `run_blocking()`：把 SQLite/算法等阻塞计算丢到 `spawn_blocking` 池 |
| `response.rs` | 统一 `AppError` + `ok()` JSON 响应包装 |

### 中间件栈（按外层→内层）

CORS → Compression（gzip/br）→ Trace → CatchPanic → 安全响应头（CSP / HSTS / X-Frame-Options / nosniff / Referrer-Policy）→ Router。

## AMAS 模块（`src/amas/`）

```
amas/
├── config.rs           # AMASConfig：16 个子配置块（feature_flags / ensemble / modeling / …）
├── engine.rs           # AMASEngine：process_event / get_config / reload_config / is_healthy
├── types.rs            # RawEvent / ProcessResult / StrategyParams / UserState 等
├── decision/           # 决策算法层
│   ├── heuristic.rs    # 经验规则
│   ├── ige.rs          # 智能梯度（Intelligent Gradient Estimator）
│   ├── swd.rs          # 间隔加权（Spaced Weighted Decay）
│   └── ensemble.rs     # 信任分加权融合 + fallback
├── memory/             # 记忆模型层
│   ├── mdm.rs          # 多模型记忆曲线 (Multi-Decay Model)
│   ├── mastery.rs      # 单词掌握度状态
│   ├── iad.rs          # 干扰感知衰减 (Interference-Aware Decay)
│   ├── mtp.rs          # 间隔预测
│   ├── ssp.rs          # 间隔策略预计算表
│   └── evm.rs          # 期望值模型
├── elo.rs              # ELO 评分 + ZPD 优先级
├── metrics.rs          # 内存级指标注册表
├── monitoring.rs       # 事件采样落库
└── tuning_whitelist.rs # 可调参数白名单 + 安全区间
```

事件流：`POST /api/amas/process-event` → `AMASEngine::process_event()` → 内部按 user 加锁 → 特征向量 → 候选策略生成（heuristic + IGE + SWD）→ 信任分加权融合 → 约束兜底 → 持久化 + 监控采样。

## Admin GUI（`admin-ui/`，后端内嵌）

本节描述的是**后端的内嵌运维 GUI**，不是独立前端项目。源码在 `admin-ui/`，构建产物落仓库根 `static/`，由后端 `tower-http::fs::ServeDir` 作为 fallback 服务出去 —— 生产环境用户永远不会单独访问 `admin-ui` 这个 npm 项目，它没有独立部署形态。

| 选型 | 版本 |
|---|---|
| 框架 | SolidJS 1.9 |
| 路由 | `@solidjs/router` |
| 数据层 | `@tanstack/solid-query` |
| 样式 | Tailwind CSS v4（`@tailwindcss/vite` 插件） |
| 构建 | Vite 7 |
| 图表 | ECharts 6 |
| 疲劳检测 | `@mediapipe/tasks-vision` + 本仓 `crates/visual-fatigue-wasm`（wasm-pack 产物） |
| 测试 | Vitest（单测）+ Playwright（E2E，71 用例） |

构建产物落 `static/`，被后端的 `tower-http::fs::ServeDir` fallback 服务出去——生产环境**单二进制即包含完整 admin GUI**。

## Workers（`src/workers/`）

由 `WorkerManager` 统一调度（基于 `tokio-cron-scheduler`），仅 `WORKER_LEADER=true` 的实例运行：

| 类别 | Worker |
|---|---|
| **AMAS** | `config_watcher`（热加载）、`llm_advisor`（DeepSeek 调参建议）、`algorithm_optimization`（指标聚合）、`monitoring_aggregate` |
| **聚合统计** | `daily_aggregation`、`weekly_report`、`metrics_flush`、`health_analysis` |
| **缓存维护** | `cache_cleanup`、`confusion_pair_cache`、`session_cleanup`、`password_reset_cleanup` |
| **学习数据** | `forgetting_alert`、`delayed_reward`、`word_clustering`、`embedding_generation`、`etymology_generation` |
| **运维** | `heartbeat_watchdog`（持久跑，不归 Manager）、`update_checker`（GitHub Releases 轮询 + SSE）、`log_export` |

## 数据存储

**SQLite + WAL** 单库设计：

- 连接池：`r2d2` + `r2d2_sqlite`，`PRAGMA journal_mode=WAL` + `synchronous=NORMAL` + `foreign_keys=ON`
- 迁移：`store/migrate.rs` 用 `schema_version` 表做增量迁移（DDL 在 `schema.rs`），首次启动跑全量 DDL，老库走增量
- 备份：自更新前 `PRAGMA wal_checkpoint(TRUNCATE)` → `VACUUM INTO` 生成单文件快照

历史上有 sled 实现，已通过一次性二进制 `migrate-sled-to-sqlite` 迁出，sled 依赖被 `sled-migration` feature 隔离，默认不编译。

## 自更新链路

1. `update_checker` worker 每小时拉 GitHub Releases，结果走 `Updater` 的 ETag 缓存
2. 管理员后台 `/admin/updates` 看到新版本提醒（SSE 推送）
3. 触发 `POST /admin/updates/install` → 下载 tarball（≤ 200 MiB）→ 校验 → SQLite 备份 → 原子替换二进制 → exec 重启
4. 失败兜底：`semver` 比对禁止 downgrade（除非 `UPDATE_ALLOW_DOWNGRADE=true`），`fs2` 文件锁防并发

## 下一步

- [AMAS 入门](./amas-intro) — 16 个子配置、调参流程、版本回滚
- [API 接口对接](/api-endpoints) — REST 接口清单
