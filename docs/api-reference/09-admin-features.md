# 管理后台功能接口

所有管理后台接口均挂载在 `/api/admin` 前缀下（认证接口挂载在 `/api/admin/auth`），除特殊标注外，均需要管理员身份认证（`AdminAuthUser`）。

---

## 管理员认证

### POST `/api/admin/auth/setup`

初始化首个管理员账户。仅在系统中不存在任何管理员时可调用，内部使用事务原子性检查防止 TOCTOU。

**认证**: 无需

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| email | string | 是 | 邮箱地址 |
| password | string | 是 | 密码（需通过强度校验） |

**响应** (201):

```json
{
  "token": "jwt-token",
  "admin": {
    "id": "uuid",
    "email": "admin@example.com"
  }
}
```

---

### POST `/api/admin/auth/login`

管理员登录。支持登录失败锁定机制。

**认证**: 无需

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| email | string | 是 | 邮箱地址 |
| password | string | 是 | 密码 |

**响应**:

```json
{
  "token": "jwt-token",
  "admin": {
    "id": "uuid",
    "email": "admin@example.com"
  }
}
```

---

### POST `/api/admin/auth/logout`

注销当前管理员会话，删除对应 session。

**认证**: 需要（从请求头提取 token）

**请求体**: 无

**响应**:

```json
{ "loggedOut": true }
```

---

### GET `/api/admin/auth/verify`

验证当前管理员 token 是否有效，返回管理员基本信息。

**认证**: 需要

**响应**:

```json
{
  "id": "uuid",
  "email": "admin@example.com"
}
```

---

### GET `/api/admin/auth/status`

查询系统是否已初始化管理员账户。此接口不受认证速率限制约束。

**认证**: 无需

**响应**:

```json
{ "initialized": true }
```

---

## 用户管理

### GET `/api/admin/users`

分页查询用户列表，支持搜索和封禁状态过滤。

**认证**: 需要

**查询参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| page | u64 | 否 | 页码，默认 1 |
| perPage | u64 | 否 | 每页数量，默认取配置值，上限取配置最大值 |
| search | string | 否 | 按用户名或邮箱模糊搜索（不区分大小写） |
| banned | bool | 否 | 按封禁状态过滤 |

**响应**: 分页格式，数据项结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 用户 ID |
| email | string | 邮箱 |
| username | string | 用户名 |
| isBanned | bool | 是否被封禁 |
| createdAt | datetime | 创建时间 |
| updatedAt | datetime | 更新时间 |
| failedLoginCount | u32 | 登录失败次数 |
| lockedUntil | datetime? | 锁定到期时间 |

---

### POST `/api/admin/users/:id/ban`

封禁指定用户并撤销其所有活跃会话。

**认证**: 需要

**路径参数**: `id` — 用户 ID

**请求体**: 无

**响应**:

```json
{
  "banned": true,
  "userId": "user-id",
  "sessionsRevoked": 3
}
```

---

### POST `/api/admin/users/:id/unban`

解封指定用户。

**认证**: 需要

**路径参数**: `id` — 用户 ID

**请求体**: 无

**响应**:

```json
{
  "banned": false,
  "userId": "user-id"
}
```

---

### POST `/api/admin/users/:id/reset-password`

为指定用户生成密码重置密钥（有效期 4 小时）。

**认证**: 需要

**路径参数**: `id` — 用户 ID

**请求体**: 无

**响应**:

```json
{
  "resetKey": "uuid-simple-format",
  "expiresInHours": 4
}
```

---

### POST `/api/admin/users/:id/set-password`

直接为指定用户设置新密码并撤销其所有会话。

**认证**: 需要

**路径参数**: `id` — 用户 ID

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| newPassword | string | 是 | 新密码（需通过强度校验） |

**响应**:

```json
{
  "passwordReset": true,
  "userId": "user-id",
  "sessionsRevoked": 2
}
```

---

### GET `/api/admin/stats`

获取系统基础统计数据。

**认证**: 需要

**响应**:

```json
{
  "users": 150,
  "words": 5000,
  "records": 30000
}
```

---

## 数据分析

### GET `/api/admin/analytics/engagement`

用户参与度分析。

**认证**: 需要

**响应**:

```json
{
  "totalUsers": 150,
  "activeToday": 42,
  "retentionRate": 0.28
}
```

---

### GET `/api/admin/analytics/learning`

学习数据统计。

**认证**: 需要

**响应**:

```json
{
  "totalWords": 5000,
  "totalRecords": 30000,
  "totalCorrect": 24000,
  "overallAccuracy": 0.8
}
```

---

## 系统监控

### GET `/api/admin/monitoring/health`

系统健康状态检查。

**认证**: 需要

**响应**:

```json
{
  "status": "healthy",
  "storeProbeOk": true,
  "dbSizeBytes": 10485760,
  "uptimeSecs": 86400,
  "version": "v0.1.3"
}
```

---

### GET `/api/admin/monitoring/database`

数据库统计信息。

**认证**: 需要

**响应**:

```json
{
  "sizeOnDisk": 10485760,
  "tableCount": 12,
  "tables": ["users", "words", "records"]
}
```

---

### GET `/api/admin/monitoring/check-update`

检查是否有新版本可用。结果缓存 1 小时。

**认证**: 需要

**响应**:

```json
{
  "currentVersion": "v0.1.3",
  "latestVersion": "0.1.4",
  "hasUpdate": true,
  "releaseUrl": "https://github.com/...",
  "releaseNotes": "..."
}
```

---

## 系统广播

### POST `/api/admin/broadcast`

向所有用户发送系统广播通知。分批处理避免内存溢出。

**认证**: 需要

**请求体**:

| 字段 | 类型 | 必填 | 校验规则 |
|------|------|------|----------|
| title | string | 是 | 1-200 字符 |
| message | string | 是 | 1-10000 字符 |

**响应**:

```json
{
  "sent": 150,
  "broadcastId": "uuid"
}
```

---

### POST `/api/admin/broadcast-update`

通过 SSE 广播版本更新通知。

**认证**: 需要

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| version | string? | 否 | 版本号 |
| message | string? | 否 | 更新说明 |

**响应**:

```json
{ "broadcasted": true }
```

---

## 客户端管理

### GET `/api/admin/clients`

获取客户端列表，包含 SSE 实时连接和最近活跃设备。

**认证**: 需要

**响应**:

```json
{
  "sseLive": [
    {
      "deviceId": "device-abc",
      "platform": "ios",
      "userId": "user-123",
      "connectedSecs": 3600,
      "connectionCount": 1,
      "isBanned": false
    }
  ],
  "recentlyActive": [
    {
      "deviceId": "device-xyz",
      "platform": "android",
      "userId": "user-456",
      "lastSeenAt": "2026-04-12T10:00:00Z",
      "isBanned": false
    }
  ]
}
```

---

### POST `/api/admin/clients/:id/ban`

封禁指定设备。封禁后通过 SSE 实时通知客户端（保持连接以支持即时解封）。

**认证**: 需要

**路径参数**: `id` — 设备 ID

**请求体**（可选）:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| reason | string? | 否 | 封禁原因，最长 500 字符 |

**响应**:

```json
{ "banned": true, "deviceId": "device-abc" }
```

---

### POST `/api/admin/clients/:id/unban`

解封指定设备，通过 SSE 实时通知客户端。

**认证**: 需要

**路径参数**: `id` — 设备 ID

**请求体**: 无

**响应**:

```json
{ "banned": false, "deviceId": "device-abc" }
```

---

### POST `/api/admin/clients/:id/request-telemetry`

向在线设备请求遥测数据。设备必须有活跃 SSE 连接。

**认证**: 需要

**路径参数**: `id` — 设备 ID

**请求体**: 无

**响应**:

```json
{ "requestId": "uuid" }
```

**错误**: 设备离线时返回 422 `DEVICE_OFFLINE`

---

### GET `/api/admin/telemetry/:device_id`

查询指定设备的遥测数据记录。

**认证**: 需要

**路径参数**: `device_id` — 设备 ID

**查询参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| limit | u32 | 否 | 返回条数，默认 50，上限 200 |
| offset | u32 | 否 | 偏移量，默认 0 |

**响应**:

```json
{
  "records": [...],
  "total": 120
}
```

---

## 系统设置

### GET `/api/admin/settings`

获取当前系统设置。

**认证**: 需要

**响应**: 系统设置对象（结构由 `SystemSettings` store 定义）

---

### PUT `/api/admin/settings`

更新系统设置，所有字段均为可选（部分更新）。

**认证**: 需要

**请求体**:

| 字段 | 类型 | 必填 | 校验规则 |
|------|------|------|----------|
| maxUsers | u64 | 否 | 1-1,000,000 |
| registrationEnabled | bool | 否 | — |
| maintenanceMode | bool | 否 | 变更时立即生效 |
| defaultDailyWords | u32 | 否 | 1-500 |
| wordbookCenterUrl | string | 否 | 空字符串清除 |

**响应**: 更新后的完整系统设置对象

---

### POST `/api/admin/settings/reload-amas`

热重载 AMAS 配置。

**认证**: 需要

**请求体**: 完整的 `AMASConfig` 对象（结构见 [AMAS 配置管理](#amas-配置管理) 节）

**响应**: 重载后的完整 AMAS 配置对象

---

## AMAS 配置管理

以下接口挂载在 `/api/admin/amas` 前缀下。

### GET `/api/admin/amas/config`

获取当前 AMAS 配置。

**认证**: 需要

**响应**: 完整的 `AMASConfig` JSON 对象，包含以下子配置：

| 子配置 | 说明 |
|--------|------|
| featureFlags | 功能开关（ensemble/heuristic/ige/swd/mdm/iad/mtp/ssp） |
| ensemble | 集成学习权重配置 |
| modeling | 行为建模参数（注意力、疲劳、动机等） |
| constraints | 约束阈值 |
| monitoring | 监控采样率与刷新间隔 |
| coldStart | 冷启动阶段转换阈值 |
| objectiveWeights | 多目标优化权重 |
| reward | 奖励/惩罚参数 |
| feature | 特征工程参数 |
| elo | Elo 评分系统参数 |
| fatigueDecay | 疲劳衰减配置 |
| heuristic | 启发式策略参数 |
| ige | IGE 探索策略参数 |
| swd | SWD 相似性策略参数 |
| memoryModel | 记忆模型参数（含 FSRS-5） |
| iad | 混淆词干扰衰减参数 |
| mtp | 词素迁移预测参数 |
| wordSelector | 单词选择器参数 |
| intervention | 干预阈值配置 |
| learningStrategy | 学习策略调整参数 |
| classifier | 学习者分类器参数 |
| ssp | SSP-MMC 最优间隔调度参数 |

---

### PUT `/api/admin/amas/config`

更新 AMAS 配置。提交前会执行参数范围校验。

**认证**: 需要

**请求体**: 完整的 `AMASConfig` 对象

**响应**:

```json
{ "updated": true }
```

---

### GET `/api/admin/amas/metrics`

获取 AMAS 指标注册表快照。

**认证**: 需要

**响应**: 指标快照数据

---

### GET `/api/admin/amas/monitoring`

查询最近的 AMAS 监控事件。

**认证**: 需要

**查询参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| limit | usize | 否 | 返回条数，默认 50，范围 1-500 |

**响应**: 监控事件列表

---

## AMAS 用户接口

以下接口挂载在 `/api/amas` 前缀下，面向普通用户（`AuthUser`）。

### POST `/api/amas/process-event`

处理单个学习事件。

**认证**: 用户认证

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| wordId | string | 是 | 单词 ID |
| isCorrect | bool | 是 | 是否答对 |
| responseTime | i64 | 是 | 响应时间（毫秒），别名 `response_time` |
| sessionId | string | 否 | 学习会话 ID |
| isQuit | bool | 否 | 是否退出，默认 false |
| dwellTime | i64 | 否 | 停留时间（毫秒） |
| pauseCount | i32 | 否 | 暂停次数 |
| switchCount | i32 | 否 | 切换次数 |
| retryCount | i32 | 否 | 重试次数 |
| focusLossDuration | i64 | 否 | 焦点丢失时长（毫秒） |
| interactionDensity | f64 | 否 | 交互密度 |
| pausedTimeMs | i64 | 否 | 暂停时长（毫秒） |
| hintUsed | bool | 否 | 是否使用提示，默认 false |

**响应**: AMAS 处理结果

---

### POST `/api/amas/batch-process`

批量处理学习事件。

**认证**: 用户认证

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| events | ProcessEventRequest[] | 是 | 事件数组，上限取配置 `limits.max_batch_size` |

**响应**:

```json
{
  "count": 5,
  "items": [...]
}
```

---

### GET `/api/amas/state`

获取当前用户的 AMAS 状态。

**认证**: 用户认证

**响应**: 用户 AMAS 状态对象

---

### GET `/api/amas/strategy`

获取当前用户的学习策略推荐。

**认证**: 用户认证

**响应**: 策略推荐对象

---

### GET `/api/amas/phase`

获取当前用户的冷启动阶段。

**认证**: 用户认证

**响应**:

```json
{ "phase": "explore" }
```

---

### GET `/api/amas/learning-curve`

获取当前用户的学习曲线数据（按天聚合，最近 1000 条记录）。

**认证**: 用户认证

**响应**:

```json
{
  "curve": [
    {
      "date": "2026-04-10",
      "total": 50,
      "correct": 42,
      "accuracy": 0.84
    }
  ]
}
```

---

### GET `/api/amas/intervention`

获取当前用户的干预建议。根据疲劳、动机、注意力阈值生成。

**认证**: 用户认证

**响应**:

```json
{
  "interventions": [
    {
      "type": "rest",
      "message": "您似乎有些疲劳，建议休息一下",
      "severity": "warning"
    }
  ]
}
```

干预类型：`rest`（疲劳）、`encouragement`（动机低）、`focus`（注意力下降）、`continue`（状态良好）

---

### POST `/api/amas/reset`

重置当前用户的 AMAS 状态。

**认证**: 用户认证

**请求体**: 无

**响应**:

```json
{ "reset": true }
```

---

### GET `/api/amas/mastery/evaluate`

评估指定单词的掌握度。

**认证**: 用户认证

**查询参数**:

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| wordId | string | 是 | 单词 ID |

**响应**:

```json
{
  "wordId": "word-123",
  "state": "REVIEWING",
  "masteryLevel": 0.75,
  "correctStreak": 3,
  "totalAttempts": 10,
  "nextReviewDate": "2026-04-15T00:00:00Z"
}
```

未学习过的单词返回 `state: "NEW"`, `masteryLevel: 0.0`。

---

### POST `/api/amas/visual-fatigue`

上报视觉疲劳分数。

**认证**: 用户认证

**请求体**:

| 字段 | 类型 | 必填 | 校验规则 |
|------|------|------|----------|
| score | f64 | 是 | 0-100 |

**响应**: 更新后的用户 AMAS 状态对象
