# 环境变量配置

项目通过 `.env` 文件管理运行时配置。首次使用请复制模板：

```bash
cp .env.example .env
```

## 核心变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `HOST` | 监听地址 | `127.0.0.1` |
| `PORT` | 监听端口 | `3000` |
| `DATABASE_URL` | SQLite 数据库路径 | `./data/learning.db` |
| `JWT_SECRET` | 用户 JWT 密钥 | **必须设置** |
| `ADMIN_JWT_SECRET` | 管理员 JWT 密钥 | **必须设置** |
| `REFRESH_JWT_SECRET` | Refresh Token 密钥 | 未设置则自动从 JWT_SECRET 派生 |
| `JWT_EXPIRES_IN_HOURS` | Access Token 有效期（小时） | `24` |
| `REFRESH_TOKEN_EXPIRES_IN_HOURS` | Refresh Token 有效期（小时） | `168` |
| `ADMIN_JWT_EXPIRES_IN_HOURS` | 管理员 Token 有效期（小时） | `2` |
| `CORS_ORIGIN` | 允许的跨域来源 | `http://localhost:5173` |
| `RUST_LOG` | 日志级别 | `info` |
| `RUST_ENV` | 运行环境 | `development` |

## 数据库配置

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `SQLITE_BUSY_TIMEOUT_MS` | SQLite 忙等待超时（毫秒） | `5000` |
| `SQLITE_POOL_SIZE` | 连接池大小 | `4` |

## 功能开关

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `API_ONLY` | 仅启动 API 服务（不托管前端静态文件） | `false` |
| `WORKER_LEADER` | 是否运行后台任务 | `true` |
| `AMAS_ENSEMBLE_ENABLED` | 启用集成记忆模型 | `true` |
| `AMAS_MONITOR_SAMPLE_RATE` | 引擎监控采样率 | `0.05` |
| `ENABLE_FILE_LOGS` | 启用文件日志 | `false` |
| `LOG_DIR` | 日志文件目录 | `./logs` |
| `TRUST_PROXY` | 信任代理头（获取真实 IP） | `false` |
| `ENABLE_LLM_ADVISOR_WORKER` | 启用 LLM 顾问 Worker | `false` |
| `ENABLE_ENGINE_MONITORING_WORKER` | 启用引擎监控 Worker | `true` |

## LLM 配置

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `LLM_ENABLED` | 启用 LLM 功能 | `false` |
| `LLM_MOCK` | 使用 Mock 模式 | `true` |
| `LLM_API_URL` | LLM API 地址 | — |
| `LLM_API_KEY` | LLM API 密钥 | — |
| `LLM_TIMEOUT_SECS` | LLM 请求超时（秒） | `30` |

## 速率限制

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `RATE_LIMIT_WINDOW_SECS` | 全局速率限制窗口（秒） | `900` |
| `RATE_LIMIT_MAX` | 窗口内最大请求数 | `500` |
| `AUTH_RATE_LIMIT_WINDOW_SECS` | 认证接口速率限制窗口（秒） | `60` |
| `AUTH_RATE_LIMIT_MAX` | 认证接口窗口内最大请求数 | `10` |

## 安全提示

JWT 密钥必须为强随机值，推荐使用以下命令生成：

```bash
openssl rand -hex 32
```

- `JWT_SECRET` 和 `ADMIN_JWT_SECRET` 为**必须设置**项，使用默认值会导致启动失败
- 三个 JWT 密钥应各不相同
- `REFRESH_JWT_SECRET` 未设置时会自动从 `JWT_SECRET` 通过 HMAC-SHA256 派生
- 生产环境下 `CORS_ORIGIN` 应设置为具体域名，避免使用 `*`
