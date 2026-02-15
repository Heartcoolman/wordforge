# 技术栈

## 后端

| 层级 | 技术 |
|------|------|
| 框架 | Axum 0.7 + Tokio 异步运行时 |
| 存储 | sled 0.34 嵌入式 KV 数据库（零依赖部署） |
| 认证 | JWT (jsonwebtoken) + Argon2 密码哈希 + HttpOnly Cookie |
| 安全 | SHA-256 Token 哈希、速率限制、CORS、请求体大小限制 |
| 调度 | tokio-cron-scheduler 后台任务系统（17+ 定时任务） |
| 日志 | tracing + tracing-subscriber（支持文件轮转） |

## 前端

| 层级 | 技术 |
|------|------|
| 框架 | SolidJS 1.9 + TypeScript 5.9 |
| 构建 | Vite 7 + vite-plugin-solid |
| 样式 | TailwindCSS 4（暗色/亮色主题） |
| 路由 | @solidjs/router |
| 状态 | SolidJS 原生响应式 (createSignal / createStore) |
| 视觉 | MediaPipe Tasks Vision（WebAssembly 疲劳检测） |
| 测试 | Vitest + @solidjs/testing-library + Playwright E2E |

## 部署架构

前端构建产物输出到 `static/` 目录，由后端 Axum `ServeDir` 托管并提供 SPA fallback，实现同源部署、零 CORS 配置。
