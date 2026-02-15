# 环境变量配置

项目通过 `.env` 文件管理运行时配置。首次使用请复制模板：

```bash
cp .env.example .env
```

## 变量列表

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

## 安全提示

JWT 密钥必须为强随机值，推荐使用以下命令生成：

```bash
openssl rand -hex 32
```

三个 JWT 密钥（`JWT_SECRET`、`ADMIN_JWT_SECRET`、`REFRESH_JWT_SECRET`）必须各不相同。
