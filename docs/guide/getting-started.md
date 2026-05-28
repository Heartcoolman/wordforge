# 快速开始

## 前置要求

- **Rust** ≥ 1.77（edition 2021）
- **Node.js** ≥ 18（前端构建 + 文档站）
- **wasm-pack**（仅在重建 `crates/visual-fatigue-wasm` 时需要）

SQLite 走 `rusqlite` 的 `bundled` feature，**不需要**单独安装 sqlite3。

## 一次跑起来

```bash
git clone https://github.com/Heartcoolman/wordforge.git && cd wordforge
cp .env.example .env

# 生成强密钥（三个必须互不相同）
openssl rand -hex 32   # → JWT_SECRET
openssl rand -hex 32   # → ADMIN_JWT_SECRET
openssl rand -hex 32   # → REFRESH_JWT_SECRET
# 把生成结果写入 .env

# 构建管理后台（产物落到 ../static/，被后端内嵌做 fallback）
cd admin-ui && npm install && npm run build && cd ..

# 启动后端
cargo run    # 监听 http://127.0.0.1:3000
```

启动后访问 `http://127.0.0.1:3000/admin` 进入管理后台。健康检查：`GET /health`。

## 开发模式（前端热更新）

```bash
cargo run                          # 终端 1：后端 :3000
cd admin-ui && npx vite --host     # 终端 2：前端 :5173，自动代理 /api → :3000
```

`admin-ui/vite.config.ts` 已配好 `/api`、`/health` 反代到 `:3000`。

## 关键环境变量

完整列表见仓库根 `.env.example`，最常用的几项：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `HOST` / `PORT` | `127.0.0.1` / `3000` | 监听地址 |
| `DATABASE_URL` | `./data/learning.db` | SQLite 文件路径，WAL 模式 |
| `JWT_SECRET` / `ADMIN_JWT_SECRET` / `REFRESH_JWT_SECRET` | **必须改** | 用 `openssl rand -hex 32` 生成，三者互不相同 |
| `CORS_ORIGIN` | `http://localhost:5173` | 允许的前端来源 |
| `WORKER_LEADER` | `true` | 多副本部署时仅一个实例置 true 跑后台 worker |
| `LLM_ENABLED` / `LLM_MOCK` | `false` / `true` | 开启 LLM 调参顾问 + 是否走 mock |
| `ENABLE_UPDATE_CHECKER_WORKER` | `true` | 后台定时拉 GitHub Releases，缓存到内存 + SSE 提醒前端 |

未开启 `LLM_ENABLED` 时，AMAS 仍然完整工作，仅 LLM 调参顾问功能不启动。

## 测试与构建

```bash
./run-all-tests.sh                  # 后端 + 前端全套
cargo test                          # 仅后端单测 + 集成测试
cd admin-ui && npm test             # 前端 Vitest
cd admin-ui && npm run test:e2e     # Playwright E2E
make coverage                       # 覆盖率报告（cargo-llvm-cov）
cargo build --release               # 生产构建：strip + LTO，单文件 ~30MB
```

## 数据目录

默认结构：

```
./data/
  ├── learning.db           # SQLite 主库（WAL 模式）
  ├── learning.db-wal       # WAL 日志
  └── learning.db-shm       # 共享内存索引
./logs/                     # ENABLE_FILE_LOGS=true 时生效
./amas_config.toml          # AMAS 16 个子配置的 TOML；首次启动自动生成
./static/                   # 前端构建产物（被二进制内嵌做 fallback）
```

`amas_config.toml` 启动时若不存在会自动写出默认值；改动会被 `config_watcher` worker 在 500ms 防抖后自动热加载——**无需重启**。

## 下一步

- [架构概览](./architecture) — 模块怎么拆、请求怎么走、后台任务有哪些
- [AMAS 入门](./amas-intro) — 调参、版本管理、LLM 顾问
