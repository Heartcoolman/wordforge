# 管理后台核心接口

管理后台核心接口涵盖管理员认证、用户管理、客户端设备管理和系统设置。所有接口（除认证状态查询外）均需管理员身份验证。

路由前缀：`/api/admin`，认证路由挂载在 `/api/admin/auth`。

---

## 认证

认证路由挂载在 `/api/admin/auth`，其中写操作（setup/login/logout）受独立的认证速率限制，`/status` 不受速率限制。

### GET `/api/admin/auth/status`

查询管理员账户是否已初始化。无需认证。

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `initialized` | `bool` | 是否已存在管理员账户 |

---

### POST `/api/admin/auth/setup`

初始化首个管理员账户。仅在无管理员存在时可用，通过事务保证原子性。

**认证**：无需

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `email` | `String` | 是 | 管理员邮箱 |
| `password` | `String` | 是 | 管理员密码 |

**响应体**（HTTP 201）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `token` | `String` | JWT token |
| `admin.id` | `String` | 管理员 ID |
| `admin.email` | `String` | 管理员邮箱 |

**错误码**：
- `ADMIN_INVALID_EMAIL` — 邮箱格式无效
- `ADMIN_WEAK_PASSWORD` — 密码强度不足
- `ADMIN_ALREADY_EXISTS` — 管理员已存在（409）

---

### POST `/api/admin/auth/login`

管理员登录。支持登录失败计数与账户临时锁定。

**认证**：无需

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `email` | `String` | 是 | 管理员邮箱 |
| `password` | `String` | 是 | 管理员密码 |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `token` | `String` | JWT token |
| `admin.id` | `String` | 管理员 ID |
| `admin.email` | `String` | 管理员邮箱 |

**错误码**：
- `401` — 邮箱或密码错误
- `429` — 账户因多次登录失败已被临时锁定

---

### GET `/api/admin/auth/verify`

验证当前管理员 token 是否有效，返回管理员基本信息。

**认证**：需要（Admin JWT）

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `String` | 管理员 ID |
| `email` | `String` | 管理员邮箱 |

---

### POST `/api/admin/auth/logout`

管理员登出，删除当前会话。

**认证**：需要（Admin JWT）

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `loggedOut` | `bool` | 始终为 `true` |

---

## 用户管理

### GET `/api/admin/users`

分页列出所有用户，支持搜索和封禁状态过滤。

**认证**：需要（Admin JWT）

**查询参数**：

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `page` | `u64` | `1` | 页码 |
| `perPage` | `u64` | 配置默认值 | 每页条数（受配置最大值限制） |
| `search` | `String` | — | 按用户名/邮箱模糊搜索（不区分大小写） |
| `banned` | `bool` | — | 按封禁状态过滤 |

**响应体**（分页格式）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `data[]` | `AdminUserView[]` | 用户列表 |
| `total` | `u64` | 总数 |
| `page` | `u64` | 当前页 |
| `perPage` | `u64` | 每页条数 |

`AdminUserView` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `String` | 用户 ID |
| `email` | `String` | 邮箱 |
| `username` | `String` | 用户名 |
| `isBanned` | `bool` | 是否被封禁 |
| `createdAt` | `DateTime` | 创建时间 |
| `updatedAt` | `DateTime` | 更新时间 |
| `failedLoginCount` | `u32` | 登录失败次数 |
| `lockedUntil` | `DateTime?` | 锁定截止时间 |

---

### POST `/api/admin/users/:id/ban`

封禁用户，同时撤销其所有活跃会话。

**认证**：需要（Admin JWT）

**路径参数**：`id` — 用户 ID

**请求体**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `banned` | `bool` | `true` |
| `userId` | `String` | 用户 ID |
| `sessionsRevoked` | `usize` | 已撤销的会话数 |

---

### POST `/api/admin/users/:id/unban`

解封用户。

**认证**：需要（Admin JWT）

**路径参数**：`id` — 用户 ID

**请求体**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `banned` | `bool` | `false` |
| `userId` | `String` | 用户 ID |

---

### POST `/api/admin/users/:id/reset-password`

为指定用户生成密码重置令牌（有效期 4 小时）。

**认证**：需要（Admin JWT）

**路径参数**：`id` — 用户 ID

**请求体**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `resetKey` | `String` | 密码重置令牌（明文，仅此次返回） |
| `expiresInHours` | `u32` | 过期时间（小时），固定为 `4` |

---

### POST `/api/admin/users/:id/set-password`

管理员直接重置用户密码，同时撤销其所有活跃会话。

**认证**：需要（Admin JWT）

**路径参数**：`id` — 用户 ID

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `newPassword` | `String` | 是 | 新密码（需满足密码强度要求） |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `passwordReset` | `bool` | `true` |
| `userId` | `String` | 用户 ID |
| `sessionsRevoked` | `usize` | 已撤销的会话数 |

**错误码**：
- `AUTH_WEAK_PASSWORD` — 密码强度不足

---

## 统计概览

### GET `/api/admin/stats`

获取系统统计数据。

**认证**：需要（Admin JWT）

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `users` | `usize` | 用户总数 |
| `words` | `usize` | 单词总数 |
| `records` | `usize` | 学习记录总数 |

---

## 客户端设备管理

路由前缀：`/api/admin/clients`

### GET `/api/admin/clients`

获取客户端设备列表，包含 SSE 实时连接和最近活跃设备（15 分钟内）。

**认证**：需要（Admin JWT）

**请求参数**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `sseLive[]` | `SseLiveEntry[]` | 当前 SSE 实时连接列表 |
| `recentlyActive[]` | `RecentlyActiveEntry[]` | 最近 15 分钟活跃设备 |

`SseLiveEntry` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `deviceId` | `String` | 设备 ID |
| `platform` | `String` | 平台 |
| `userId` | `String` | 用户 ID |
| `connectedSecs` | `u64` | 已连接秒数 |
| `connectionCount` | `usize` | 连接数 |
| `isBanned` | `bool` | 是否被封禁 |

`RecentlyActiveEntry` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `deviceId` | `String` | 设备 ID |
| `platform` | `String` | 平台 |
| `userId` | `String?` | 用户 ID |
| `lastSeenAt` | `String` | 最后活跃时间 |
| `isBanned` | `bool` | 是否被封禁 |

---

### POST `/api/admin/clients/:id/ban`

封禁设备。封禁后通过 SSE 实时通知客户端，但保持连接以支持即时解封。

**认证**：需要（Admin JWT）

**路径参数**：`id` — 设备 ID

**请求体**（可选）：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `reason` | `String` | 否 | 封禁原因（最多 500 字符） |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `banned` | `bool` | `true` |
| `deviceId` | `String` | 设备 ID |

---

### POST `/api/admin/clients/:id/unban`

解封设备，通过 SSE 实时通知客户端。

**认证**：需要（Admin JWT）

**路径参数**：`id` — 设备 ID

**请求体**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `banned` | `bool` | `false` |
| `deviceId` | `String` | 设备 ID |

---

### POST `/api/admin/clients/:id/request-telemetry`

向在线设备请求遥测数据上报。设备必须有活跃 SSE 连接。

**认证**：需要（Admin JWT）

**路径参数**：`id` — 设备 ID

**请求体**：无

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `requestId` | `String` | 遥测请求 ID |

**错误码**：
- `DEVICE_OFFLINE`（422）— 设备当前无活跃 SSE 连接

---

### GET `/api/admin/telemetry/:device_id`

分页获取指定设备的遥测记录。

**认证**：需要（Admin JWT）

**路径参数**：`device_id` — 设备 ID

**查询参数**：

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `limit` | `u32` | `50` | 每页条数（最大 200） |
| `offset` | `u32` | `0` | 偏移量 |

**响应体**：

| 字段 | 类型 | 说明 |
|------|------|------|
| `records` | `Array` | 遥测记录列表 |
| `total` | `u64` | 总记录数 |

---

## 系统设置

路由前缀：`/api/admin/settings`

### GET `/api/admin/settings`

获取当前系统设置。

**认证**：需要（Admin JWT）

**请求参数**：无

**响应体**（`SystemSettings`）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `maxUsers` | `u64` | 最大用户数 |
| `registrationEnabled` | `bool` | 是否开放注册 |
| `maintenanceMode` | `bool` | 是否处于维护模式 |
| `defaultDailyWords` | `u32` | 每日默认单词数 |
| `wordbookCenterUrl` | `String?` | 词书中心 URL |

---

### PUT `/api/admin/settings`

更新系统设置，所有字段均为可选，仅更新传入字段。更新 `maintenanceMode` 时会实时生效。

**认证**：需要（Admin JWT）

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `maxUsers` | `u64` | 否 | 最大用户数（1 - 1,000,000） |
| `registrationEnabled` | `bool` | 否 | 是否开放注册 |
| `maintenanceMode` | `bool` | 否 | 是否启用维护模式 |
| `defaultDailyWords` | `u32` | 否 | 每日默认单词数（1 - 500） |
| `wordbookCenterUrl` | `String` | 否 | 词书中心 URL（空字符串清除） |

**响应体**：更新后的 `SystemSettings`（同 GET 响应）

**错误码**：
- `INVALID_MAX_USERS` — 最大用户数超出范围
- `INVALID_DAILY_WORDS` — 每日单词数超出范围

---

### POST `/api/admin/settings/reload-amas`

热重载 AMAS（自适应记忆调度系统）配置。需提交完整 `AMASConfig` 对象，服务端会进行参数校验。

**认证**：需要（Admin JWT）

**请求体**：完整的 `AMASConfig` JSON 对象，包含以下顶层字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `featureFlags` | `FeatureFlags` | 功能开关 |
| `ensemble` | `EnsembleConfig` | 集成学习权重配置 |
| `modeling` | `ModelingConfig` | 用户建模参数 |
| `constraints` | `ConstraintConfig` | 约束条件 |
| `monitoring` | `MonitoringConfig` | 监控配置 |
| `coldStart` | `ColdStartConfig` | 冷启动配置 |
| `objectiveWeights` | `ObjectiveWeights` | 目标权重 |
| `reward` | `RewardConfig` | 奖励函数配置 |
| `feature` | `FeatureConfig` | 特征提取配置 |
| `elo` | `EloConfig` | ELO 评分配置 |
| `fatigueDecay` | `FatigueDecayConfig` | 疲劳衰减配置 |
| `heuristic` | `HeuristicConfig` | 启发式策略配置 |
| `ige` | `IgeConfig` | IGE 策略配置 |
| `swd` | `SwdConfig` | SWD 策略配置 |
| `memoryModel` | `MemoryModelConfig` | 记忆模型配置 |
| `iad` | `IadConfig` | 干扰感知衰减配置 |
| `mtp` | `MtpConfig` | 词素迁移预测配置 |
| `wordSelector` | `WordSelectorConfig` | 单词选择器配置 |
| `intervention` | `InterventionConfig` | 干预阈值配置 |
| `learningStrategy` | `LearningStrategyConfig` | 学习策略配置 |
| `classifier` | `ClassifierConfig` | 学习者分类器配置 |
| `ssp` | `SspConfig` | SSP 最优间隔调度配置 |

**响应体**：当前生效的 `AMASConfig` 对象

**错误码**：
- `INVALID_AMAS_CONFIG` — 配置参数校验失败
