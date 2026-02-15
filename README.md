# WordForge — 智能英语学习系统

一个基于自适应算法驱动的全栈英语学习平台，采用 **Rust Axum** 后端 + **SolidJS** 前端架构，内置 AMAS（Adaptive Mastery Acquisition System）自适应掌握度习得系统。

**[在线文档](https://heartcoolman.github.io/wordforge/)**

---

## 核心特性

- **AMAS 自适应算法** — 融合 ELO 评分与多模型记忆曲线，实时调整学习节奏
- **智能选词** — 基于遗忘概率、难度匹配和学习阶段的多维度选词
- **疲劳感知** — MediaPipe + WebAssembly 摄像头疲劳检测，自动降低强度
- **词书中心** — 在线浏览、导入、同步词书资源，支持自定义词本
- **数据洞察** — 学习统计、历史回溯、遗忘预警
- **管理后台** — 用户管理、系统监控、AMAS 配置、全局广播

---

## 快速开始

**前置要求**：Rust 1.77+、Node.js 18+

```bash
git clone <repo-url> english && cd english
cp .env.example .env
# 编辑 .env，替换 JWT_SECRET / ADMIN_JWT_SECRET / REFRESH_JWT_SECRET 为强随机值

cd frontend && npm install && npm run build && cd ..
cargo run    # http://127.0.0.1:3000
```

开发模式（热更新）：

```bash
cargo run                          # 终端 1
cd frontend && npx vite --host     # 终端 2 → http://localhost:5173
```

---

## 测试

```bash
./run-all-tests.sh                 # 全部测试
cd frontend && npm test            # 前端单元测试
cd frontend && npm run test:e2e    # E2E 测试（71 用例）
make coverage                      # 覆盖率报告
```

---

## 许可证

Private — All rights reserved.
