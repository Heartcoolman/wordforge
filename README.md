# WordForge

**Adaptive vocabulary learning platform powered by AMAS (Adaptive Mastery Acquisition System).**

Self-hosted, single-binary, SQLite-backed — no Redis, no queues. Real-time word scheduling that adjusts on every answer event.

---

## WordForge 是什么

WordForge 是一个**自适应算法驱动的英语词汇学习平台**，由 Rust 后端 + **内嵌的 SolidJS 管理 GUI**（`admin-ui/`，构建产物作为单二进制的静态资源 fallback）+ 独立的用户学习端（[wordforge-web](https://github.com/Heartcoolman/wordforge-web)）组成。

> **架构关键事实**：admin GUI 不是独立的客户端 deliverable，构建产物直接打进 `learning-backend` 二进制（落 `static/` 由 `tower-http::ServeDir` 服务）；end-user 学习端 `wordforge-web` 是另一个独立仓库，与本仓无构建依赖。

核心引擎 **AMAS** 在每次答题事件后实时调整后续选词、间隔与节奏，融合记忆曲线（MDM）、ELO 评分与疲劳感知三个维度。

### 关键特性

| 模块 | 说明 |
|---|---|
| **AMAS 引擎** | 16 个子配置 / 6 类决策算法（ensemble / heuristic / IGE / SWD / MDM / SSP）+ ELO + 疲劳衰减 |
| **智能选词** | 遗忘概率（MDM）× 难度匹配（ELO）× 学习阶段（冷启动 → 稳定）三维评估 |
| **疲劳感知** | MediaPipe + WebAssembly 摄像头检测，信号注入 AMAS 强度调节 |
| **LLM 调参顾问** | 每 20 分钟跑一次 DeepSeek，产出参数 patch，白名单 + 成本上限 + 灰度自动应用 |
| **配置热加载** | `amas_config.toml` 500ms 防抖 + validate → 原子生效，无需重启 |
| **管理后台** | 用户 / 广播 / AMAS 调参 / 版本对比 / 监控 / 自更新；SolidJS + Tailwind v4 |
| **自更新** | GitHub Releases tarball，原子替换 + SQLite `VACUUM INTO` 备份，admin 后台一键触发 |

---

## 架构概览

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

详细架构说明见 [docs/guide/architecture.md](docs/guide/architecture.md)。

---

## 一键安装（Linux 生产服务器）

```bash
# 需要 root，支持 x86_64 / aarch64
sudo bash <(curl -fsSL https://raw.githubusercontent.com/Heartcoolman/wordforge/main/install.sh)
```

安装后服务监听 `http://127.0.0.1:3000`，管理后台入口：`http://<your-server>:3000/admin`。

> **国内网络**：如拉取 GitHub 较慢，可先手动下载 release tarball 后执行 `install.sh`，或通过 [ghproxy.net](https://ghproxy.net) 镜像拉取。

---

## 本地开发快速开始

**前置条件**：Rust ≥ 1.77、Node.js ≥ 18、wasm-pack（仅重建 visual-fatigue-wasm 时需要）

```bash
git clone https://github.com/Heartcoolman/wordforge.git && cd wordforge
cp .env.example .env

# 生成强密钥（三个必须互不相同）
openssl rand -hex 32   # → JWT_SECRET
openssl rand -hex 32   # → ADMIN_JWT_SECRET
openssl rand -hex 32   # → REFRESH_JWT_SECRET
# 把生成结果写入 .env

# 构建管理后台前端
cd frontend && npm install && npm run build && cd ..

# 启动后端（监听 http://127.0.0.1:3000）
cargo run
```

启动后访问 `http://127.0.0.1:3000/admin` 进入管理后台。

### 开发模式（前端热更新）

```bash
cargo run                          # 终端 1：后端 :3000
cd frontend && npx vite --host     # 终端 2：前端 :5173，代理 /api → :3000
```

### 运行测试

```bash
./run-all-tests.sh          # 后端 + 前端全套
cargo test                  # 仅后端
cd frontend && npm test     # 前端 Vitest
```

---

## 文档站

完整文档部署于 GitHub Pages（VitePress）：

**https://heartcoolman.github.io/wordforge/**

- [项目简介](https://heartcoolman.github.io/wordforge/guide/introduction)
- [架构概览](https://heartcoolman.github.io/wordforge/guide/architecture)
- [AMAS 入门](https://heartcoolman.github.io/wordforge/guide/amas-intro)
- [快速开始](https://heartcoolman.github.io/wordforge/guide/getting-started)
- [API 接口对接](https://heartcoolman.github.io/wordforge/api-endpoints)

---

## 许可证

MIT License — 详见 [LICENSE](LICENSE)

---

## 贡献

欢迎 issue 与 PR。参与前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。

安全漏洞请通过 [SECURITY.md](SECURITY.md) 中的私密渠道报告，**不要**开公开 issue。
