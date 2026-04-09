# 客户端对接指南

## 认证

WordForge 使用 JWT 认证，采用 access/refresh token 轮换机制。

### 流程

```
POST /api/auth/register  ──┐
POST /api/auth/login     ──┤
                           ├──> accessToken + refreshToken（Set-Cookie + JSON body）
                           │
携带 accessToken ──────────┤──> Authorization: Bearer <accessToken>
                           │    或 Cookie: token=<accessToken>
                           │
Token 过期 (401) ──────────┤──> POST /api/auth/refresh
                           │    Authorization: Bearer <refreshToken>
                           │    返回新 token 对（旧 refresh token 失效）
                           │
POST /api/auth/logout ─────┘──> 撤销所有会话，清除 Cookie
```

### Token 传递

Token 同时通过 JSON 响应体和 `Set-Cookie` 头传递：

- `token` cookie — access token（HttpOnly, Secure, SameSite=Strict）
- `refresh_token` cookie — refresh token（HttpOnly, Secure, SameSite=Strict）

原生/移动端客户端不使用 Cookie 时，从 JSON 响应体提取 `accessToken`，通过 `Authorization` 头携带。

### Token 刷新

Refresh token 为一次性使用。每次调用 `/api/auth/refresh` 会使旧 refresh token 失效并返回新 token 对。重放旧 refresh token 会被拒绝。

### 管理员认证

管理员端点使用独立的 JWT secret（`ADMIN_JWT_SECRET`）。管理员 token 从 `/api/admin/auth/login` 获取，与用户 token 不可互换。

---

## 响应格式

### 成功

```json
{
  "success": true,
  "data": { ... }
}
```

### 分页

```json
{
  "success": true,
  "data": {
    "data": [ ... ],
    "total": 100,
    "page": 1,
    "perPage": 20,
    "totalPages": 5
  }
}
```

### 错误

```json
{
  "success": false,
  "code": "AUTH_UNAUTHORIZED",
  "message": "可读的错误描述",
  "traceId": "可选的追踪 ID"
}
```

---

## 错误处理

| 状态码 | code 模式 | 处理方式 |
|--------|----------|---------|
| 400 | `AUTH_INVALID_EMAIL`、`BATCH_TOO_LARGE` 等 | 修正请求后重试 |
| 401 | `AUTH_UNAUTHORIZED` | 刷新 token；刷新也失败则重新登录 |
| 403 | `FORBIDDEN` | 无权限（被封禁、角色不匹配、非资源所有者） |
| 404 | `NOT_FOUND` | 资源不存在 |
| 409 | `AUTH_EMAIL_EXISTS`、`WB_CENTER_ALREADY_IMPORTED` | 资源已存在 |
| 413 | `PAYLOAD_TOO_LARGE` | 缩减请求体大小（限制：2 MiB） |
| 422 | 校验错误 | 按 message 描述修正输入 |
| 429 | `RATE_LIMITED` | 退避重试，等待 `Retry-After` 头指定的秒数 |
| 500 | `INTERNAL_ERROR` | 服务端错误；message 始终为通用文案 |

### 速率限制

- 认证端点（`/api/auth/*`、`/api/admin/auth/*`）有更严格的 IP 级别限制
- 通用 API 端点共享全局限制
- 被限速时检查 `Retry-After` 响应头获取冷却秒数
- 账户锁定：多次登录失败后账户临时锁定（返回 429）

---

## 分页

分页端点接受 `page` 和 `perPage` 查询参数：

```
GET /api/words?page=2&perPage=20
```

- `page` 默认 1，最小 1
- `perPage` 默认 20，最大 100（不同端点可能不同）
- 响应的 `data` 中包含 `total`、`page`、`perPage`、`totalPages`

部分端点使用 `limit` / `offset`（如到期词列表使用 `limit`）。

---

## 学习流程

核心学习循环：

```
1. POST /api/learning/session
   -> { sessionId, resumed, targetMasteryCount }

2. GET  /api/learning/study-words
   -> { words: [...], strategy: { difficultyRange, newRatio, batchSize } }

3. 用户作答 -> POST /api/records
   Body: { wordId, isCorrect, responseTimeMs, sessionId, ... }
   -> { record, amasResult, duplicate }
   （AMAS 引擎自动更新单词状态和会话计数器）

4. 需要更多单词 -> POST /api/learning/next-words
   Body: { excludeWordIds, masteredWordIds, sessionPerformance }

5. 完成会话 -> POST /api/learning/complete-session
   Body: { sessionId, masteredWordIds, errorProneWordIds, avgResponseTimeMs }
```

### 幂等性

- `POST /api/records` 支持 `clientRecordId` 实现幂等提交。相同 clientRecordId 的记录已存在时返回已有记录和 `duplicate: true`，不触发 AMAS 处理。
- `POST /api/v1/records` 按 word + correctness 在 5 秒窗口内去重。

---

## SSE 实时事件

连接 SSE 端点接收 AMAS 状态实时更新：

```
GET /api/realtime/events
Authorization: Bearer <accessToken>
```

事件类型：

| 事件类型 | 数据 | 说明 |
|---------|------|------|
| `amas_state` | `{ type, attention, fatigue, motivation, confidence, sessionEventCount, totalEventCount }` | AMAS 用户状态变更（每 5 秒轮询） |

保活：服务器每 15 秒发送 `keepalive` 注释。

连接限制：服务器限制最大并发 SSE 连接数，超出返回 429。

### 客户端实现示例

```javascript
const es = new EventSource('/api/realtime/events', {
  headers: { 'Authorization': `Bearer ${token}` }
});

es.addEventListener('amas_state', (e) => {
  const state = JSON.parse(e.data);
  // 用 state.attention, state.fatigue 等更新 UI
});

es.onerror = () => {
  // 指数退避重连
};
```

---

## 移动端注意事项

### CORS

服务器从 `CORS_ORIGIN` 环境变量读取允许的 origin。Capacitor 应用需设置：

```
CORS_ORIGIN=capacitor://localhost
```

多 origin 用逗号分隔：`CORS_ORIGIN=https://web.app.com,capacitor://localhost`

### CSP

CSP 头在服务端配置。`connect-src` 已包含 `capacitor:` 和 `ionic:` scheme。

### Cookie 处理

原生 HTTP 客户端可能不自动处理 `Set-Cookie`。此时：

1. 从 JSON body 提取 `accessToken`
2. 安全存储 token（iOS: Keychain，Android: EncryptedSharedPreferences）
3. 每次请求通过 `Authorization: Bearer <token>` 头携带

### 离线处理

- 使用 `clientRecordId` 在重连后安全重放排队的作答记录
- `POST /api/records/batch` 支持批量提交，部分失败时检查响应中的 `errors` 数组

---

## V1 兼容层

`/api/v1/*` 路由提供不含 AMAS 引擎的简化 API：

- `GET /api/v1/words` — 列出单词
- `GET /api/v1/words/:id` — 获取单词
- `GET /api/v1/records` — 列出记录
- `POST /api/v1/records` — 提交记录（无 AMAS 处理）
- `GET /api/v1/study-config` — 获取学习配置
- `POST /api/v1/learning/session` — 创建/恢复会话

适用于只需基础 CRUD 而不需要自适应学习的轻量客户端。
