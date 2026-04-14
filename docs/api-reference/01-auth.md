# 认证接口

所有认证接口的基础路径为 `/api/auth`，并受认证专用速率限制中间件保护。

认证令牌通过以下两种方式传递：
- `Authorization: Bearer <token>` 请求头
- `token` / `refresh_token` Cookie（HttpOnly, Secure, SameSite=Strict）

成功登录/注册/刷新时，响应会同时通过 JSON body 和 `Set-Cookie` 头返回令牌。

---

### POST /api/auth/register

**描述**: 注册新用户账号，返回访问令牌和用户信息。

**认证**: 不需要

**前置条件**: 系统注册功能开启、非维护模式、用户总数未达上限。

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| email | string | 是 | 邮箱地址，需符合格式校验 |
| username | string | 是 | 用户名，需符合长度和字符校验 |
| password | string | 是 | 密码，需符合强度校验 |

**响应体** (201 Created):

| 字段 | 类型 | 说明 |
|------|------|------|
| accessToken | string | JWT 访问令牌 |
| user | object | 用户信息 |
| user.id | string | 用户 ID (UUID) |
| user.email | string | 邮箱 |
| user.username | string | 用户名 |
| user.isBanned | boolean | 是否被封禁 |

**错误码**:
- `AUTH_INVALID_EMAIL` — 邮箱格式无效
- `AUTH_INVALID_USERNAME` — 用户名不符合要求
- `AUTH_WEAK_PASSWORD` — 密码强度不足
- `AUTH_EMAIL_EXISTS` — 邮箱已被注册

---

### POST /api/auth/login

**描述**: 用户登录，验证邮箱和密码后返回访问令牌和用户信息。

**认证**: 不需要

**前置条件**: 非维护模式。

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| email | string | 是 | 注册邮箱 |
| password | string | 是 | 密码 |

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| accessToken | string | JWT 访问令牌 |
| user | object | 用户信息 |
| user.id | string | 用户 ID |
| user.email | string | 邮箱 |
| user.username | string | 用户名 |
| user.isBanned | boolean | 是否被封禁 |

**错误码**:
- `401` — 邮箱或密码错误
- `403` — 用户已被封禁
- `429` — 账户因多次登录失败被临时锁定

**安全机制**: 当邮箱不存在时仍执行 dummy 哈希验证，防止基于响应时间的枚举攻击。

---

### POST /api/auth/refresh

**描述**: 使用刷新令牌获取新的访问令牌和刷新令牌对。旧的刷新令牌在使用后立即失效（轮换机制）。

**认证**: 需要刷新令牌（通过 `Authorization: Bearer` 头、`refresh_token` Cookie 或 `token` Cookie）

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| accessToken | string | 新的 JWT 访问令牌 |
| user | object | 用户信息 |
| user.id | string | 用户 ID |
| user.email | string | 邮箱 |
| user.username | string | 用户名 |
| user.isBanned | boolean | 是否被封禁 |

**错误码**:
- `401` — 刷新令牌无效、已过期、已被使用或会话不匹配
- `403` — 用户已被封禁

**安全机制**: 刷新令牌一次性使用，重放将被拒绝。

---

### POST /api/auth/logout

**描述**: 登出当前用户，撤销该用户所有会话并清除认证 Cookie。

**认证**: 需要用户访问令牌（`AuthUser`）

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| loggedOut | boolean | 固定为 `true` |

---

### POST /api/auth/forgot-password

**描述**: 请求密码重置。无论邮箱是否存在，均返回相同的成功响应以防止用户枚举。生成的重置令牌有效期 1 小时。

**认证**: 不需要

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| email | string | 是 | 注册邮箱 |

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| emailSent | boolean | 固定为 `true` |
| message | string | 提示信息 |

---

### POST /api/auth/reset-password

**描述**: 使用重置令牌设置新密码。令牌一次性使用，成功后撤销该用户所有会话。

**认证**: 不需要（通过请求体中的重置令牌验证身份）

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| token | string | 是 | 密码重置令牌 |
| newPassword | string | 是 | 新密码，需符合强度校验 |

**响应体** (200 OK): 空对象 `{}`

**错误码**:
- `AUTH_WEAK_PASSWORD` — 新密码强度不足
- `AUTH_INVALID_RESET_TOKEN` — 重置令牌无效
- `AUTH_EXPIRED_RESET_TOKEN` — 重置令牌已过期

---

### POST /api/auth/verify-reset-token

**描述**: 验证密码重置令牌是否有效且未过期（不消耗令牌）。

**认证**: 不需要

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| token | string | 是 | 待验证的重置令牌 |

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| valid | boolean | 固定为 `true` |

**错误码**:
- `AUTH_INVALID_RESET_TOKEN` — 令牌无效
- `AUTH_EXPIRED_RESET_TOKEN` — 令牌已过期
