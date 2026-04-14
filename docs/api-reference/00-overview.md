# API 总览

## API 基础信息

| 项目 | 说明 |
|------|------|
| 基础路径 | `/api` |
| 版本策略 | 主路由无版本前缀；兼容层通过 `/api/v1` 提供轻量级映射 |
| 请求体限制 | 2 MiB（遥测接口为 64 KB） |
| 内容格式 | JSON（`application/json`） |
| 认证方式 | JWT Bearer Token 或 HttpOnly Cookie |

## 路由树总览

### 用户认证

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/auth/register` | POST | 无 | 用户注册 |
| `/api/auth/login` | POST | 无 | 用户登录 |
| `/api/auth/refresh` | POST | Refresh Token | 刷新令牌 |
| `/api/auth/logout` | POST | AuthUser | 退出登录 |
| `/api/auth/forgot-password` | POST | 无 | 忘记密码 |
| `/api/auth/reset-password` | POST | 无 | 重置密码 |
| `/api/auth/verify-reset-token` | POST | 无 | 验证重置令牌 |

> `/api/auth` 路由组附加认证专用速率限制（`auth_rate_limit_middleware`）。

### 用户管理

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/users/me` | GET | AuthUser | 获取当前用户信息 |
| `/api/users/me` | PUT | AuthUser | 更新用户名 |
| `/api/users/me/password` | PUT | AuthUser | 修改密码 |
| `/api/users/me/stats` | GET | AuthUser | 用户学习统计 |

### 用户画像

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/user-profile/reward` | GET/PUT | AuthUser | 奖励偏好 |
| `/api/user-profile/cognitive` | GET | AuthUser | 认知画像 |
| `/api/user-profile/learning-style` | GET | AuthUser | 学习风格 |
| `/api/user-profile/chronotype` | GET | AuthUser | 时间类型 |
| `/api/user-profile/habit` | GET/POST | AuthUser | 学习习惯 |
| `/api/user-profile/avatar` | POST | AuthUser | 上传头像（512KB 上限） |

### 单词管理

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/words` | GET | AuthUser | 单词列表（支持搜索） |
| `/api/words` | POST | AdminAuthUser | 创建单词 |
| `/api/words/count` | GET | AuthUser | 单词总数 |
| `/api/words/batch` | POST | AdminAuthUser | 批量创建单词 |
| `/api/words/batch-get` | POST | AuthUser | 批量获取单词 |
| `/api/words/import-url` | POST | AdminAuthUser | 从 URL 导入单词 |
| `/api/words/:id` | GET | AuthUser | 单词详情 |
| `/api/words/:id` | PUT | AdminAuthUser | 更新单词 |
| `/api/words/:id` | DELETE | AdminAuthUser | 删除单词 |

### 单词学习状态

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/word-states/batch` | POST | AuthUser | 批量查询学习状态 |
| `/api/word-states/due/list` | GET | AuthUser | 到期复习列表 |
| `/api/word-states/stats/overview` | GET | AuthUser | 状态统计概览 |
| `/api/word-states/batch-update` | POST | AuthUser | 批量更新状态 |
| `/api/word-states/:word_id` | GET | AuthUser | 单词学习状态 |
| `/api/word-states/:word_id/mark-mastered` | POST | AuthUser | 标记已掌握 |
| `/api/word-states/:word_id/reset` | POST | AuthUser | 重置学习状态 |

### 学习记录

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/records` | GET | AuthUser | 学习记录列表 |
| `/api/records` | POST | AuthUser | 提交学习记录（触发 AMAS 引擎） |
| `/api/records/statistics` | GET | AuthUser | 基础统计 |
| `/api/records/statistics/enhanced` | GET | AuthUser | 增强统计（含每日明细） |
| `/api/records/batch` | POST | AuthUser | 批量提交记录 |

### 学习会话与选词

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/learning/session` | POST | AuthUser | 创建或恢复学习会话 |
| `/api/learning/study-words` | GET | AuthUser | 获取学习单词（AMAS 策略选词） |
| `/api/learning/next-words` | POST | AuthUser | 获取下一批单词 |
| `/api/learning/adjust-words` | POST | AuthUser | 动态调整学习策略 |
| `/api/learning/sync-progress` | POST | AuthUser | 同步会话进度 |
| `/api/learning/complete-session` | POST | AuthUser | 完成学习会话 |
| `/api/learning/pick-next-word` | POST | AuthUser | 选取下一个单词 |
| `/api/learning/generate-options` | POST | AuthUser | 生成选项题 |

### 学习配置

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/study-config` | GET | AuthUser | 获取学习配置 |
| `/api/study-config` | PUT | AuthUser | 更新学习配置 |
| `/api/study-config/today-words` | GET | AuthUser | 今日待学单词 |
| `/api/study-config/progress` | GET | AuthUser | 学习进度 |

### AMAS 自适应引擎（用户端）

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/amas/process-event` | POST | AuthUser | 处理学习事件 |
| `/api/amas/batch-process` | POST | AuthUser | 批量处理事件 |
| `/api/amas/state` | GET | AuthUser | 用户 AMAS 状态 |
| `/api/amas/strategy` | GET | AuthUser | 当前学习策略 |
| `/api/amas/phase` | GET | AuthUser | 当前学习阶段 |
| `/api/amas/learning-curve` | GET | AuthUser | 学习曲线数据 |
| `/api/amas/intervention` | GET | AuthUser | 干预建议 |
| `/api/amas/reset` | POST | AuthUser | 重置 AMAS 状态 |
| `/api/amas/mastery/evaluate` | GET | AuthUser | 单词掌握度评估 |
| `/api/amas/visual-fatigue` | POST | AuthUser | 上报视觉疲劳 |

### 词书管理

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/wordbooks/system` | GET | AuthUser | 系统词书列表 |
| `/api/wordbooks/user` | GET | AuthUser | 用户词书列表 |
| `/api/wordbooks` | POST | AuthUser | 创建用户词书 |
| `/api/wordbooks/:id/words` | GET | AuthUser | 词书单词列表 |
| `/api/wordbooks/:id/words` | POST | AuthUser | 向词书添加单词 |
| `/api/wordbooks/:id/words/:word_id` | DELETE | AuthUser | 从词书移除单词 |

### 词书中心（用户端）

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/wordbook-center/browse` | GET | AuthUser | 浏览远程词书 |
| `/api/wordbook-center/browse/:id` | GET | AuthUser | 预览远程词书 |
| `/api/wordbook-center/import/:id` | POST | AuthUser | 导入远程词书 |
| `/api/wordbook-center/import-url` | POST | AuthUser | 从 URL 导入词书 |
| `/api/wordbook-center/updates` | GET | AuthUser | 可用更新列表 |
| `/api/wordbook-center/updates/:id/sync` | POST | AuthUser | 同步词书更新 |
| `/api/wordbook-center/settings` | GET/PUT | AuthUser | 词书中心设置 |

### 通知与偏好

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/notifications` | GET | AuthUser | 通知列表 |
| `/api/notifications/unread-count` | GET | AuthUser | 未读通知数 |
| `/api/notifications/:id/read` | PUT | AuthUser | 标记已读 |
| `/api/notifications/read-all` | POST | AuthUser | 全部标记已读 |
| `/api/notifications/badges` | GET | AuthUser | 成就徽章列表 |
| `/api/notifications/preferences` | GET/PUT | AuthUser | 用户偏好设置 |

### 内容服务

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/content/etymology/:word_id` | GET | AuthUser | 词源信息 |
| `/api/content/semantic/search` | GET | AuthUser | 语义搜索 |
| `/api/content/word-contexts/:word_id` | GET | AuthUser | 单词上下文 |
| `/api/content/morphemes/:word_id` | GET | AuthUser | 获取词素 |
| `/api/content/morphemes/:word_id` | POST | AdminAuthUser | 设置词素 |
| `/api/content/confusion-pairs/:word_id` | GET | AuthUser | 易混淆词对 |

### 实时通信

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/realtime/events` | GET | AuthUser | SSE 事件流 |

### 遥测

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/telemetry` | POST | AuthUser | 提交遥测数据（64KB 限制） |

### 状态查询

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/status` | GET | 无 | 系统状态（维护模式、版本） |
| `/api/status/device-ban` | GET | 无 | 设备封禁查询 |

### V1 兼容层

V1 路由提供轻量级兼容映射，不触发 AMAS 引擎。

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/v1/words` | GET | AuthUser | 单词列表 |
| `/api/v1/words/:id` | GET | AuthUser | 单词详情 |
| `/api/v1/records` | GET | AuthUser | 学习记录列表 |
| `/api/v1/records` | POST | AuthUser | 提交记录（无 AMAS） |
| `/api/v1/study-config` | GET | AuthUser | 学习配置 |
| `/api/v1/learning/session` | POST | AuthUser | 创建学习会话 |

### 管理后台认证

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/admin/auth/status` | GET | 无 | 管理员是否已初始化 |
| `/api/admin/auth/setup` | POST | 无 | 初始化管理员账户 |
| `/api/admin/auth/login` | POST | 无 | 管理员登录 |
| `/api/admin/auth/logout` | POST | AdminAuthUser | 管理员退出 |
| `/api/admin/auth/verify` | GET | AdminAuthUser | 验证管理员令牌 |

> `/api/admin/auth` 路由组（除 `/status` 外）附加认证专用速率限制。

### 管理后台功能

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/api/admin/users` | GET | AdminAuthUser | 用户列表（支持搜索、筛选） |
| `/api/admin/users/:id/ban` | POST | AdminAuthUser | 封禁用户 |
| `/api/admin/users/:id/unban` | POST | AdminAuthUser | 解封用户 |
| `/api/admin/users/:id/reset-password` | POST | AdminAuthUser | 生成密码重置密钥 |
| `/api/admin/users/:id/set-password` | POST | AdminAuthUser | 直接重置用户密码 |
| `/api/admin/stats` | GET | AdminAuthUser | 系统统计概要 |
| `/api/admin/analytics/engagement` | GET | AdminAuthUser | 用户活跃度分析 |
| `/api/admin/analytics/learning` | GET | AdminAuthUser | 学习指标分析 |
| `/api/admin/monitoring/health` | GET | AdminAuthUser | 系统健康状态 |
| `/api/admin/monitoring/database` | GET | AdminAuthUser | 数据库统计 |
| `/api/admin/monitoring/check-update` | GET | AdminAuthUser | 检查版本更新 |
| `/api/admin/settings` | GET/PUT | AdminAuthUser | 系统设置 |
| `/api/admin/settings/reload-amas` | POST | AdminAuthUser | 重载 AMAS 配置 |
| `/api/admin/broadcast` | POST | AdminAuthUser | 系统广播通知 |
| `/api/admin/broadcast-update` | POST | AdminAuthUser | 广播更新通知 |
| `/api/admin/clients` | GET | AdminAuthUser | 客户端设备列表 |
| `/api/admin/clients/:id/ban` | POST | AdminAuthUser | 封禁设备 |
| `/api/admin/clients/:id/unban` | POST | AdminAuthUser | 解封设备 |
| `/api/admin/clients/:id/request-telemetry` | POST | AdminAuthUser | 请求设备遥测 |
| `/api/admin/telemetry/:device_id` | GET | AdminAuthUser | 查询设备遥测数据 |
| `/api/admin/amas/config` | GET/PUT | AdminAuthUser | AMAS 配置管理 |
| `/api/admin/amas/metrics` | GET | AdminAuthUser | AMAS 指标 |
| `/api/admin/amas/monitoring` | GET | AdminAuthUser | AMAS 监控事件 |
| `/api/admin/wordbook-center/browse` | GET | AdminAuthUser | 浏览远程词书 |
| `/api/admin/wordbook-center/browse/:id` | GET | AdminAuthUser | 预览远程词书 |
| `/api/admin/wordbook-center/import/:id` | POST | AdminAuthUser | 导入为系统词书 |
| `/api/admin/wordbook-center/updates` | GET | AdminAuthUser | 可用更新列表 |
| `/api/admin/wordbook-center/updates/:id/sync` | POST | AdminAuthUser | 同步词书更新 |

### 健康检查（不经过 `/api` 前缀）

| 路径 | 方法 | 认证 | 说明 |
|------|------|------|------|
| `/health` | GET | 无 | 基础健康检查 |
| `/health/live` | GET | 无 | 存活探针 |
| `/health/ready` | GET | 无 | 就绪探针 |
| `/health/database` | GET | AdminAuthUser | 数据库健康（含延迟） |
| `/health/metrics` | GET | AdminAuthUser | AMAS 算法指标 |

## 中间件说明

中间件按 Axum 洋葱模型从外到内执行。以下为 `/api` 路由组的中间件栈（从外层到内层）：

| 层级 | 中间件 | 作用范围 | 说明 |
|------|--------|----------|------|
| 1 | `request_id_middleware` | 全局（含 `/health`） | 生成或透传 `X-Request-Id`；记录请求日志（方法、路径、状态码、延迟）；为错误响应注入 `traceId` |
| 2 | `SetResponseHeaderLayer` | 全局 | 安全头：`X-Content-Type-Options`、`X-Frame-Options`、`Referrer-Policy`、`Content-Security-Policy`、`Strict-Transport-Security` |
| 3 | `CatchPanicLayer` | 全局 | 捕获 handler panic，返回 500 |
| 4 | `TraceLayer` | 全局 | tower-http 请求追踪 |
| 5 | `CompressionLayer` | 全局 | 响应压缩 |
| 6 | `CorsLayer` | 全局 | CORS 配置（支持单源、多源、通配符） |
| 7 | `DefaultBodyLimit` | `/api` | 请求体大小限制（2 MiB） |
| 8 | `rate_limit_middleware` | `/api` | 全局速率限制（基于 IP，滑动窗口，分片锁） |
| 9 | `maintenance_middleware` | `/api` | 维护模式拦截。白名单：`/admin/*`、`/status`、`/realtime/*`、`/telemetry` |
| 10 | `device_middleware` | `/api`（跳过 `/admin/*`） | 设备封禁检查；记录设备活跃状态 |
| 特殊 | `auth_rate_limit_middleware` | `/api/auth`、`/api/admin/auth`（除 `/status`） | 认证路由专用速率限制 |
| 特殊 | `static_cache_headers` | 非 API 路径 | 静态资源缓存控制 |

## 认证机制

### 用户认证（`AuthUser`）

提取器 `AuthUser` 从请求中提取并验证用户身份：

1. 从 `Authorization: Bearer <token>` 或 `token` Cookie 提取 JWT
2. 使用 `jwt_secret` 验证签名和有效期
3. 校验 `token_type == "user"`
4. 查询会话表确认 token 未被撤销
5. 校验会话归属与 JWT `sub` 一致
6. 查询用户记录，校验未被封禁

### 管理员认证（`AdminAuthUser`）

提取器 `AdminAuthUser` 从请求中提取并验证管理员身份：

1. 从 `Authorization: Bearer <token>` 或 `token` Cookie 提取 JWT
2. 使用 `admin_jwt_secret` 验证签名和有效期
3. 校验 `token_type == "admin"`
4. 查询管理员会话表确认 token 未被撤销
5. 校验会话归属与 JWT `sub` 一致

### 令牌安全

- 密码哈希：Argon2id
- JWT 算法：HS256
- Token 存储：SHA-256 哈希后存储，原文不落库
- Refresh Token：独立密钥签发，`token_type == "refresh"`，一次性使用（原子删除防重放）
- Cookie 属性：`HttpOnly; Secure; SameSite=Strict`
- 会话上限：每用户最多 10 个并发会话，超出时自动清理最旧会话

### 请求体提取器（`JsonBody`）

自定义提取器 `JsonBody<T>` 封装 `axum::Json<T>`，将反序列化错误统一转换为 `AppError` 格式返回，避免框架默认的纯文本错误响应。

## 静态资源服务

当 `api_only = false` 时，启用 SPA 静态资源服务：

- `static/` 目录作为静态文件根目录
- 所有未匹配路由 fallback 到 `static/index.html`
- HTML 响应：`Cache-Control: no-cache, must-revalidate`
- `/assets/*`：`Cache-Control: public, max-age=31536000, immutable`
- 其他静态文件：`Cache-Control: public, max-age=3600`
