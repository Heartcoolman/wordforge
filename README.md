# English — 智能英语学习系统

一个基于自适应算法驱动的全栈英语学习平台，采用 **Rust Axum** 后端 + **SolidJS** 前端架构，内置 AMAS（Adaptive Mastery Acquisition System）自适应掌握度习得系统，实现个性化、高效的词汇学习体验。

---

## 核心特性

### 学习引擎

- **AMAS 自适应算法** — 融合 ELO 评分体系与多模型记忆曲线，根据用户表现实时调整学习节奏
- **智能选词策略** — 基于遗忘概率、难度匹配和学习阶段的多维度选词
- **疲劳感知调节** — 集成 MediaPipe 摄像头疲劳检测（WebAssembly），自动降低学习强度
- **动态策略调整** — 根据用户状态（专注/疲劳）实时调整难度范围、新词比例和批量大小
- **延迟奖励机制** — 跟踪长期记忆效果，优化算法参数

### 词汇管理

- **词书中心** — 在线浏览、预览、导入和同步词书资源
- **个人词本** — 自定义词本管理，支持多词本切换
- **闪卡复习** — 传统闪卡模式，适合快速巩固
- **词汇状态追踪** — 单词级别的学习进度和掌握度记录

### 数据洞察

- **学习统计** — 可视化学习时长、正确率、词汇量增长趋势
- **学习历史** — 详细的每次学习会话记录回溯
- **通知系统** — 遗忘预警、学习提醒等智能通知

### 管理后台

- **用户管理** — 查看/搜索用户、封禁/解封、双模式密码重置（直接重置 / 密钥自助）
- **系统监控** — 健康检查、数据库状态、引擎性能指标
- **数据分析** — 用户参与度与学习效果全局统计
- **AMAS 配置** — 在线调整算法参数，实时生效
- **系统设置** — 注册开关、维护模式、用户上限等
- **全局广播** — 向全体用户推送系统通知

---

## 技术栈

### 后端

| 层级 | 技术 |
|------|------|
| 框架 | Axum 0.7 + Tokio 异步运行时 |
| 存储 | sled 0.34 嵌入式 KV 数据库（零依赖部署） |
| 认证 | JWT (jsonwebtoken) + Argon2 密码哈希 + HttpOnly Cookie |
| 安全 | SHA-256 Token 哈希、速率限制、CORS、请求体大小限制 |
| 调度 | tokio-cron-scheduler 后台任务系统（17+ 定时任务） |
| 日志 | tracing + tracing-subscriber（支持文件轮转） |

### 前端

| 层级 | 技术 |
|------|------|
| 框架 | SolidJS 1.9 + TypeScript 5.9 |
| 构建 | Vite 7 + vite-plugin-solid |
| 样式 | TailwindCSS 4（暗色/亮色主题） |
| 路由 | @solidjs/router |
| 状态 | SolidJS 原生响应式 (createSignal / createStore) |
| 视觉 | MediaPipe Tasks Vision（WebAssembly 疲劳检测） |
| 测试 | Vitest + @solidjs/testing-library + Playwright E2E |

---

## 项目结构

```
english/
├── src/                          # Rust 后端
│   ├── main.rs                   # 入口
│   ├── amas/                     # AMAS 自适应算法引擎
│   │   ├── engine.rs             #   引擎核心
│   │   ├── elo.rs                #   ELO 评分系统
│   │   ├── memory/               #   记忆模型（遗忘曲线）
│   │   ├── decision/             #   决策层
│   │   ├── word_selector.rs      #   智能选词
│   │   ├── config.rs             #   算法参数配置
│   │   ├── metrics.rs            #   性能指标
│   │   └── monitoring.rs         #   引擎监控
│   ├── routes/                   # API 路由
│   │   ├── auth.rs               #   用户认证
│   │   ├── learning.rs           #   学习流程
│   │   ├── words.rs              #   单词管理
│   │   ├── wordbooks.rs          #   词本管理
│   │   ├── wordbook_center.rs    #   词书中心
│   │   ├── records.rs            #   学习记录
│   │   ├── notifications.rs      #   通知系统
│   │   ├── realtime.rs           #   SSE 实时推送
│   │   ├── admin/                #   管理后台路由
│   │   └── ...
│   ├── workers/                  # 后台定时任务（17+）
│   │   ├── session_cleanup.rs    #   会话清理
│   │   ├── forgetting_alert.rs   #   遗忘预警
│   │   ├── daily_aggregation.rs  #   每日数据聚合
│   │   ├── delayed_reward.rs     #   延迟奖励计算
│   │   └── ...
│   ├── store/                    # 数据存储层（sled）
│   ├── middleware/               # 中间件（速率限制、请求 ID）
│   ├── services/                 # 业务服务
│   └── auth.rs                   # JWT / 密码哈希
├── frontend/                     # SolidJS 前端
│   ├── src/
│   │   ├── pages/                # 页面组件
│   │   │   ├── LearningPage.tsx  #   核心学习页面
│   │   │   ├── VocabularyPage.tsx #  词汇管理
│   │   │   ├── FlashcardPage.tsx #   闪卡复习
│   │   │   ├── StatisticsPage.tsx #  数据统计
│   │   │   ├── admin/            #   管理后台页面
│   │   │   └── ...
│   │   ├── components/           # UI 组件库
│   │   │   ├── ui/               #   通用 UI（Button, Modal, Card...）
│   │   │   ├── fatigue/          #   疲劳检测组件
│   │   │   └── layout/           #   布局组件
│   │   ├── api/                  # API 客户端
│   │   ├── stores/               # 状态管理
│   │   ├── lib/                  # 工具库
│   │   └── types/                # TypeScript 类型
│   └── tests/                    # 前端测试
├── static/                       # 静态资源 + SPA 入口
├── .env.example                  # 环境变量模板
└── Cargo.toml
```

---

## 快速开始

### 前置要求

- Rust 1.77+（`rustup update stable`）
- Node.js 18+（推荐 20+）

### 1. 克隆与配置

```bash
git clone <repo-url> english
cd english
cp .env.example .env
```

编辑 `.env`，**务必替换以下密钥为强随机值**：

```bash
# 生成密钥
openssl rand -hex 32

# 分别填入
JWT_SECRET=<生成的随机值>
ADMIN_JWT_SECRET=<另一个随机值>
REFRESH_JWT_SECRET=<再一个随机值>
```

### 2. 构建前端

```bash
cd frontend
npm install
npm run build      # 产物输出到 ../static/
cd ..
```

### 3. 启动后端

```bash
cargo run
# 服务默认监听 http://127.0.0.1:3000
```

### 4. 开发模式

同时启动后端和前端开发服务器：

```bash
# 终端 1 — 后端
cargo run

# 终端 2 — 前端（热更新）
cd frontend
npx vite --host    # http://localhost:5173
```

前端开发服务器会将 API 请求代理到后端。

---

## 认证机制

系统采用分层认证架构：

| 角色 | Access Token | Refresh Token | 存储方式 |
|------|-------------|---------------|---------|
| 用户 | JWT (短期) | JWT (长期) | Access Token 仅内存；Refresh Token 通过 HttpOnly Secure Cookie |
| 管理员 | JWT (独立密钥) | — | sessionStorage |

- 所有 Token 在服务端以 SHA-256 哈希形式存储，原文不落盘
- Refresh Token 采用一次性消费机制，防止重放攻击
- 账户锁定：连续登录失败后自动临时锁定
- 管理员首次使用需通过 `/admin/setup` 初始化

---

## AMAS 算法简介

**Adaptive Mastery Acquisition System** 是系统的核心学习引擎：

```
┌─────────────┐
│  用户作答    │
└──────┬──────┘
       ▼
┌──────────────┐     ┌────────────────┐
│  ELO 评分    │────▶│  记忆模型更新   │
│  (难度匹配)  │     │  (遗忘概率预测) │
└──────┬───────┘     └───────┬────────┘
       ▼                     ▼
┌──────────────────────────────────┐
│        智能选词决策引擎           │
│  综合：遗忘概率 × 难度匹配 ×     │
│        学习阶段 × 疲劳状态       │
└──────────────┬───────────────────┘
               ▼
┌──────────────────────┐
│  下一批学习词汇       │
│  (动态调整批量大小)   │
└──────────────────────┘
```

- **ELO 系统**：借鉴国际象棋评分，为用户和单词分别维护评分，实现精准难度匹配
- **多模型记忆曲线**：支持多种遗忘模型的集成预测
- **实时策略调整**：根据用户疲劳度和近期表现，动态调节新词比例、难度区间和每批词量

---

## 后台任务

系统内置 17+ 定时后台任务，自动维护数据质量和学习效果：

| 任务 | 功能 |
|------|------|
| `session_cleanup` | 清理过期会话 |
| `password_reset_cleanup` | 清理过期密码重置令牌 |
| `forgetting_alert` | 生成遗忘预警通知 |
| `daily_aggregation` | 每日学习数据聚合 |
| `weekly_report` | 周度学习报告生成 |
| `delayed_reward` | 延迟奖励信号计算 |
| `metrics_flush` | 引擎指标持久化 |
| `cache_cleanup` | 缓存数据清理 |
| `algorithm_optimization` | 算法参数自优化 |
| `health_analysis` | 系统健康分析 |
| `monitoring_aggregate` | 监控数据聚合 |
| `log_export` | 日志导出 |
| ... | ... |

---

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `HOST` | 监听地址 | `127.0.0.1` |
| `PORT` | 监听端口 | `3000` |
| `SLED_PATH` | 数据库路径 | `./data/learning.sled` |
| `JWT_SECRET` | 用户 JWT 密钥 | **必须设置** |
| `ADMIN_JWT_SECRET` | 管理员 JWT 密钥 | **必须设置** |
| `REFRESH_JWT_SECRET` | Refresh Token 密钥 | **必须设置** |
| `JWT_EXPIRES_IN_HOURS` | Access Token 有效期 | `24` |
| `CORS_ORIGIN` | 允许的跨域来源 | `http://localhost:5173` |
| `RUST_LOG` | 日志级别 | `info` |
| `WORKER_LEADER` | 是否运行后台任务 | `true` |
| `AMAS_ENSEMBLE_ENABLED` | 启用集成记忆模型 | `true` |
| `ENABLE_FILE_LOGS` | 启用文件日志 | `false` |
| `RUST_ENV` | 运行环境 | `development` |

---

## 测试

项目包含完整的测试套件：

### 快速测试

```bash
# 运行所有测试（推荐）
./run-all-tests.sh

# 仅前端测试
cd frontend && npm test

# 仅后端测试
JWT_SECRET="test_secret" ADMIN_JWT_SECRET="test_admin_secret" cargo test
```

### 单元测试

**后端测试 (Rust)**

```bash
# 运行所有测试
JWT_SECRET="test_secret_key_for_jwt_signing_minimum_64_characters_long_abcd" \
ADMIN_JWT_SECRET="test_admin_secret_key_for_jwt_signing_minimum_64_chars_long" \
cargo test

# 运行特定测试
cargo test --test auth_tests
```

**前端测试 (Vitest)**

```bash
cd frontend

# 运行测试
npm test

# 监听模式
npm run test:watch

# 生成覆盖率报告
npm run test:coverage
```

### E2E测试

项目包含完整的端到端测试套件，使用 Playwright 测试框架，覆盖71个测试用例。

```bash
cd frontend

# 首次运行需要安装Playwright浏览器
npx playwright install chromium

# 运行E2E测试
npm run test:e2e

# 使用脚本运行（自动安装依赖）
./run-e2e-tests.sh

# 查看测试报告
npx playwright show-report

# 调试模式
npx playwright test --debug
```

**E2E测试覆盖模块：**
- ✅ 认证流程（登录、注册、密码重置）
- ✅ 管理后台
- ✅ 学习流程
- ✅ 词本管理
- ✅ 词书中心
- ✅ 学习配置
- ✅ 用户资料
- ✅ 通知系统
- ✅ 学习记录
- ✅ 主页和导航

详细文档见 [frontend/e2e/README.md](frontend/e2e/README.md)

### 测试覆盖率

```bash
# 生成所有覆盖率报告
make coverage

# 仅后端
make coverage-backend

# 仅前端
make coverage-frontend

# 在浏览器中查看
make coverage-open
```

---

## 许可证

Private — All rights reserved.
