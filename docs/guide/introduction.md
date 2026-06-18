# WordForge 是什么

WordForge 是一个**自适应算法驱动的英语词汇学习平台**，由 Rust 后端 + **内嵌的 SolidJS 管理 GUI**（`admin-ui/`，作为单二进制的静态资源 fallback）+ 独立的用户学习端（[wordforge-web](https://github.com/Heartcoolman/wordforge-web)，单独仓库）组成。核心是内置的 **AMAS**（Adaptive Mastery Acquisition System，自适应掌握度习得系统），它在每一次答题事件后实时调整后续选词、间隔与节奏。

## 设计目标

- **数据先行**：每次答题都进入 AMAS 事件流，记忆模型、ELO、疲劳信号同步更新，下一次选词立刻反映在结果上。
- **可观测、可回滚**：所有 AMAS 配置变更落 `amas_config_versions` 表，三档来源（管理员手动 / LLM 建议 / LLM 自动）共享同一审计 + 回滚通道。
- **就地运行**：单二进制 + SQLite（WAL）默认部署，无需 Redis / 消息队列；自更新走 GitHub Releases tarball，原子替换 + 数据库 `VACUUM INTO` 备份。
- **可调不可乱调**：所有可调参数走 `tuning_whitelist`，区间硬约束在代码里，越界值在 LLM/手动两条路径都会被拒。

## 关键特性

| 模块 | 说明 |
|---|---|
| **AMAS** | 16 个子配置 / 6 类决策算法（ensemble、heuristic、IGE、SWD、MDM、SSP）+ ELO + 疲劳衰减 |
| **智能选词** | 基于遗忘概率（MDM）、难度匹配（ELO）、学习阶段（冷启动 → 稳定）三维评估 |
| **疲劳感知** | MediaPipe + WebAssembly 摄像头检测，疲劳信号注入 AMAS 强度调节 |
| **LLM 调参顾问** | 每 20 分钟跑一次 DeepSeek，综合近期指标产出 patch 建议，白名单 + 成本上限 + 灰度自动应用 |
| **配置热加载** | `amas_config.toml` 被 `notify` watcher 盯着，500ms 防抖 + validate → `reload_config()` 原子生效 |
| **Admin GUI（`admin-ui/`）** | 用户、广播、AMAS 调参、版本对比、监控、自更新、资源包；SolidJS + Tailwind v4（数据层用 SolidJS 原生 `createResource`）；构建产物内嵌进后端二进制 |
| **资源包热更** | 签名（Ed25519）分发的不发版内容/配置热更，三端（Web / iOS / Android）共用 `content-slots` / `app-config` 两个通用容器，详见 [资源包热更](/resource-packs) |
| **API 服务** | axum 0.7 统一对外，给管理后台与移动/Web 学习端复用 |

## 仓库定位

本仓库是**后端 + 内嵌 admin GUI 的 monorepo**（**不是**典型的"前后端分离 monorepo"）。`admin-ui/` 是后端的运维 GUI 源码，构建产物 `static/` 由 `learning-backend` 二进制通过 `tower-http::ServeDir` 内嵌 fallback 服务；它**没有独立部署形态** —— 离开后端二进制就不存在。end-user 学习端 Web 已拆分到独立项目 `wordforge-web`，本仓库只对历史用户路径保留过渡页（`LegacyUserFrontendPage`）。

## 四端构成

WordForge 是「一后端 + 三客户端」的四端体系，**各端均为独立 GitHub 仓库、独立发版**；本后端仓库是对外 API 与算法/契约的**唯一权威**，三客户端通过 `/api/*` 接入：

| 端 | 仓库 | 当前版本 | 说明 |
|---|---|---|---|
| Server | `wordforge-server` | v1.2.0-beta.14 | Rust 后端 + 内嵌 admin GUI，本仓库 |
| Web | `wordforge-web` | v1.3.1 | end-user 学习端 SPA，独立部署 |
| Android | `wordforge-android` | v1.3.1 | 原生客户端 |
| iOS | `wordforge-ios` | v1.2.3 | 原生客户端 |

三客户端共享同一套契约（OpenAPI / AMAS 事件流 / 资源包签名分发 / 设备心跳 ≤ 10s 等），客户端版本节奏可各自独立，但必须满足后端契约门控（如资源包 `minAppVersion`）。

## 下一步

- 想跑起来 → [快速开始](./getting-started)
- 想看代码骨架 → [架构概览](./architecture)
- 想理解 AMAS → [AMAS 入门](./amas-intro)
