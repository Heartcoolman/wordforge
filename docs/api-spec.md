# API 对接规范文档

> 适用对象：通过 `/api/*` 接入 WordForge 后端的 **end-user 客户端**（Web、iOS、Android、[wordforge-web](https://github.com/Heartcoolman/wordforge-web) 等）。
> 本文档描述全局约定，包括认证机制、请求格式、响应结构、错误处理、限流规则等。
>
> **不在本文范围**：内嵌的 admin GUI（`/admin/*` 路由）走独立的 `/api/admin/*` + Admin Token，属于后端的运维管理面，不属于"客户端"范畴。

---

## 1. 基础约定

| 项目 | 说明 |
|---|---|
| 基础路径 | `/api` |
| 数据格式 | JSON（`Content-Type: application/json`） |
| 字段命名 | camelCase（服务端统一使用 `serde(rename_all = "camelCase")`） |
| 时间格式 | ISO 8601 UTC，例：`2024-01-15T08:30:00Z` |
| ID 格式 | UUID v4 字符串 |
| 最大请求体 | 2 MiB（超出返回 413） |
| CORS | `CORS_ORIGIN` 环境变量配置，默认 `http://localhost:5173` |

---

## 2. 认证机制

### 2.1 令牌体系

系统采用双令牌架构：

| 令牌 | 类型 | 有效期（默认） | 用途 |
|---|---|---|---|
| Access Token | HS256 JWT | 24 小时 | 访问所有受保护 API |
| Refresh Token | HS256 JWT | 7 天 | 刷新 Access Token |
| Admin Token | HS256 JWT | 2 小时 | 访问管理员 API |

**JWT Claims 结构：**
```json
{
  "sub": "<userId>",
  "token_type": "user" | "refresh" | "admin",
  "iat": 1705300200,
  "exp": 1705386600,
  "jti": "<uuid>"
}
```

### 2.2 Token 传递方式

客户端携带 Access Token 的两种方式（优先级从高到低）：

1. **HTTP Header（推荐）**
   ```
   Authorization: Bearer <accessToken>
   ```

2. **Cookie（备选）**
   ```
   Cookie: token=<accessToken>
   ```

Refresh Token 传递方式：

1. `Authorization: Bearer <refreshToken>`（刷新接口专用）
2. `Cookie: refresh_token=<refreshToken>`

### 2.3 Token 获取与刷新流程

```
注册/登录 → 返回 accessToken + 写入 Cookie(token, refresh_token)
         ↓
使用 accessToken 访问 API
         ↓
accessToken 过期 → POST /api/auth/refresh（携带 refresh_token）
         ↓
获得新的 accessToken（refresh_token 同时轮换，一次性使用）
```

**关键安全机制：**
- Refresh Token 一次性使用：每次刷新后旧 token 立即失效，防止重放攻击
- Token 以 SHA-256 哈希形式存储在数据库，原始 token 不落库
- 登录失败 5 次后账号锁定 15 分钟
- 修改密码/登出后，该用户所有 session 立即失效

---

## 3. 请求头规范

### 3.1 必须携带的请求头

| Header | 适用场景 | 说明 |
|---|---|---|
| `Content-Type: application/json` | 所有 POST/PUT/PATCH 请求 | 请求体为 JSON 时必须 |
| `Authorization: Bearer <token>` | 所有受保护端点 | 或通过 Cookie 传递 |

### 3.2 可选但推荐的请求头

| Header | 类型 | 说明 |
|---|---|---|
| `x-device-id` | string | 设备唯一标识（UUID），用于设备追踪和封禁管理 |
| `x-device-platform` | string | 设备平台，如 `web` / `ios` / `android`，默认 `unknown` |

> **注意：** 携带 `x-device-id` 的请求，若对应设备被管理员封禁，将直接返回 403。

### 3.3 服务端响应头

服务端会在响应中自动追加以下限流信息头：

| Header | 说明 |
|---|---|
| `ratelimit-limit` | 当前窗口最大请求数 |
| `ratelimit-remaining` | 当前窗口剩余可用次数 |
| `ratelimit-reset` | 窗口重置时间（Unix 时间戳） |
| `retry-after` | 当前限流窗口总秒数（仅 429 响应） |

---

## 4. 统一响应结构

### 4.1 成功响应

```json
{
  "success": true,
  "data": <T>
}
```

> `/health*` 健康检查接口不使用统一 `success/data` 包装：`/health`、`/health/database`、`/health/metrics` 直接返回各自的健康数据；`/health/live` 和 `/health/ready` 可能只返回 HTTP 状态码。

### 4.2 分页响应

```json
{
  "success": true,
  "data": {
    "data": [ ... ],
    "total": 150,
    "page": 1,
    "perPage": 20,
    "totalPages": 8
  }
}
```

### 4.3 错误响应

```json
{
  "success": false,
  "code": "AUTH_UNAUTHORIZED",
  "message": "令牌无效或已过期",
  "traceId": "f47ac10b-58cc-4372-a567-0e02b2c3d479"
}
```

> 当前中间件返回的 `CLIENT_BANNED` 和 `MAINTENANCE` 错误体不包含 `success: false`，结构为 `{"code":"...","message":"...","traceId":"..."}`。客户端应优先按 HTTP 状态码和 `code` 判断错误类型。

> 5xx 错误的 `message` 统一返回 `"服务器内部错误"`，具体原因仅记录在服务端日志中。

---

## 5. 错误码规范

### 5.1 HTTP 状态码与错误码对照

| HTTP 状态 | 错误码 | 触发场景 |
|---|---|---|
| 400 | `INVALID_REQUEST_BODY` | JSON 解析失败或 Content-Type 缺失 |
| 400 | `VALIDATION_ERROR` | 字段校验失败（来自 Store 层） |
| 400 | 业务错误码 | 各模块自定义前缀码，如 `AUTH_*` / `LEARNING_*` / `WORDS_*` / `WORDBOOK_*` / `BATCH_TOO_LARGE` 等 |
| 401 | `AUTH_UNAUTHORIZED` | 未携带 token 或 token 失效 |
| 403 | `FORBIDDEN` | 无权限、账号被封禁、注册关闭或用户数量达到上限 |
| 403 | `CLIENT_BANNED` | 设备被封禁（由中间件拦截） |
| 404 | `NOT_FOUND` | 资源不存在 |
| 409 | `CONFLICT` | 资源冲突（如邮箱已注册） |
| 413 | `PAYLOAD_TOO_LARGE` | 请求体超过 2 MiB |
| 429 | `RATE_LIMITED` | 触发通用限流或**账号因登录失败锁定** |
| 429 | `AUTH_RATE_LIMITED` | 触发认证端点限流 |
| 500 | `INTERNAL_ERROR` | 服务端内部错误（`message` 统一脱敏为 `服务器内部错误`） |
| 503 | `MAINTENANCE` | 服务器维护中（响应体结构见 §9，客户端应按 HTTP 503 或 `code=MAINTENANCE` 识别） |

> **注意**：账号被管理员封禁返回 `403 FORBIDDEN`；账号因**连续登录失败 5 次**被临时锁定则返回 `429 RATE_LIMITED`，两者不同。

### 5.2 认证错误消息参考

| 消息 | 含义 |
|---|---|
| `"缺少认证令牌"` | 未携带 Authorization 头或 token cookie |
| `"令牌无效或已过期"` | JWT 签名错误或已超期 |
| `"令牌类型无效"` | 使用了错误类型的 token（如用 refresh token 访问 API） |
| `"会话不存在或已过期"` | Token 已被撤销或数据库中无对应记录 |
| `"用户已被封禁"` | 账号被管理员封禁 |
| `"认证尝试次数过多，请稍后再试"` | 认证接口触发限流 |

---

## 6. 限流规则

### 6.1 通用限流（所有 `/api/*` 端点）

| 参数 | 默认值 | 说明 |
|---|---|---|
| 时间窗口 | 900 秒（15 分钟） | **固定窗口**（窗口到期后计数归零） |
| 最大请求数 | 500 次 | 每 IP 每窗口 |
| 超出后 HTTP 状态 | 429 | |
| 超出后错误码 | `RATE_LIMITED` | |

### 6.2 认证端点限流（更严格）

适用端点：`/api/auth/register`、`/api/auth/login`、`/api/auth/refresh`、`/api/auth/forgot-password`、`/api/auth/reset-password`、`/api/auth/verify-reset-token`

| 参数 | 默认值 |
|---|---|
| 时间窗口 | 60 秒 |
| 最大请求数 | 10 次 |
| 超出后错误码 | `AUTH_RATE_LIMITED` |

### 6.3 IP 识别规则

- 服务器配置 `TRUST_PROXY=true` 时，优先读取 `x-forwarded-for`（取第一个 IP），其次读取 `x-real-ip`
- IPv6 地址聚合至 `/64` 前缀参与限流计数

---

## 7. 分页约定

所有列表接口统一使用如下查询参数：

| 参数 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `page` | number | 1 | 页码，从 1 开始 |
| `perPage` | number | 20（`/api/v1` 记录类为 50） | 每页条数，最大 100 |

分页响应结构见第 4.2 节。响应中的 `page` 为请求传入的页码，可能大于 `totalPages`。

---

## 8. 批量操作约定

- 批量接口（`/batch`、`/batch-get` 等）的数组长度上限由服务器 `max_batch_size` 配置决定（默认 500）
- 批量操作部分失败时：HTTP 200，响应体包含 `errors` 数组指出失败条目
- 批量操作全部成功时：创建类返回 201，查询/更新类返回 200

---

## 9. 维护模式

服务器维护期间，除以下端点外均返回 `503 MAINTENANCE`：

- `/api/admin/*`
- `/api/status`
- `/api/realtime/*`
- `/api/telemetry`

响应体：
```json
{
  "code": "MAINTENANCE",
  "message": "服务器维护中，请稍后重试",
  "traceId": "f47ac10b-58cc-4372-a567-0e02b2c3d479"
}
```

> 维护响应**不包含 `success` 字段**。`traceId` 由请求 ID 中间件自动注入。客户端判断时应优先识别 HTTP 状态码 503 或 `code` 字段。

---

## 10. 数据类型约定

| 类型 | 格式 |
|---|---|
| ID | UUID v4 字符串，如 `"f47ac10b-58cc-4372-a567-0e02b2c3d479"` |
| 时间戳 | ISO 8601 UTC，如 `"2024-01-15T08:30:00Z"` |
| 浮点数 | IEEE 754 f64 |
| 概率/比率 | 0.0 ~ 1.0 的 f64 |
| 单词学习状态枚举 | `"new"` / `"learning"` / `"reviewing"` / `"mastered"` / `"forgotten"`（lowercase；v0.6.0-beta.4 P3#7 起；`WordLearningState.state` 字段）<br>注意：`WordMasteryDecision.masteryLevel` 是**不同枚举**，仍为 `"NEW"/"LEARNING"` 等 SCREAMING_SNAKE_CASE |
| 会话状态枚举 | `"active"` / `"completed"` / `"abandoned"` |
| 单词本类型枚举 | `"system"` / `"user"`（字段名为 `type`） |
| 学习记录类型枚举 | `"learning"` / `"review"` / `"all"`（字段名为 `recordType`，写入端可选；缺省落库 `"all"`） |
| 统计分类查询 | `?category=learning\|review\|all`，缺省或 `all` 等同不过滤；用于 `/api/records/statistics`、`/api/records/statistics/enhanced`、`/api/word-states/stats/overview` |
