# API 接口对接文档

> 所有接口路径均以 `/api` 为前缀（健康检查除外）。
> 认证方式、响应结构、错误码等全局约定请参见《API 对接规范文档》。
> **注意：** 服务器维护期间（503）的响应不包含 `success` 字段，结构为 `{"code":"MAINTENANCE","message":"服务器维护中，请稍后重试","traceId":"..."}`（`traceId` 由请求 ID 中间件自动注入），客户端应优先识别 HTTP 状态码 503。详见规范文档 §9。

---

## 目录

1. [认证模块 `/api/auth`](#1-认证模块)
2. [用户模块 `/api/users`](#2-用户模块)
3. [用户档案 `/api/user-profile`](#3-用户档案)
4. [单词模块 `/api/words`](#4-单词模块)
5. [学习记录 `/api/records`](#5-学习记录)
6. [学习会话 `/api/learning`](#6-学习会话)
7. [单词学习状态 `/api/word-states`](#7-单词学习状态)
8. [单词本 `/api/wordbooks`](#8-单词本)
9. [单词本中心 `/api/wordbook-center`](#9-单词本中心)
10. [学习配置 `/api/study-config`](#10-学习配置)
11. [通知与偏好 `/api/notifications`](#11-通知与偏好)
12. [内容增强 `/api/content`](#12-内容增强)
13. [AMAS 自适应引擎 `/api/amas`](#13-amas-自适应引擎)
14. [实时推送 `/api/realtime`](#14-实时推送)
15. [遥测 `/api/telemetry`](#15-遥测)
16. [状态 `/api/status`](#16-状态)
17. [健康检查 `/health`](#17-健康检查)
18. [V1 兼容层 `/api/v1`](#18-v1-兼容层)
19. [单词收藏 `/api/word-favorites`](#19-单词收藏)
20. [单词笔记 `/api/word-notes`](#20-单词笔记)

---

## 1. 认证模块

**路径前缀：** `/api/auth`
**限流：** 专用认证限流（60 秒 10 次），适用于 `/register`、`/login`、`/refresh`、`/forgot-password`、`/reset-password`、`/verify-reset-token`

---

### POST /api/auth/register

注册新账号。

**认证：** 无

**请求体：**
```json
{
  "email": "user@example.com",
  "username": "Alice",
  "password": "SecurePass1"
}
```

**成功响应 201：**
```json
{
  "success": true,
  "data": {
    "accessToken": "<jwt>",
    "user": {
      "id": "<uuid>",
      "email": "user@example.com",
      "username": "Alice",
      "isBanned": false
    }
  }
}
```

同时写入 Cookie：`token`（access token，HttpOnly）、`refresh_token`（HttpOnly）

**错误码：**

| HTTP | 错误码 | 说明 |
|---|---|---|
| 400 | `AUTH_INVALID_EMAIL` | 邮箱格式不合法 |
| 400 | `AUTH_INVALID_USERNAME` | 用户名格式不合法 |
| 400 | `AUTH_WEAK_PASSWORD` | 密码强度不足 |
| 409 | `AUTH_EMAIL_EXISTS` | 邮箱已注册 |
| 403 | `FORBIDDEN` | 注册功能已关闭或用户注册数量已达上限 |
| 503 | `MAINTENANCE` | 系统处于维护中 |

---

### POST /api/auth/login

用户登录。

**认证：** 无

**请求体：**
```json
{
  "email": "user@example.com",
  "password": "SecurePass1"
}
```

**成功响应 200：**
```json
{
  "success": true,
  "data": {
    "accessToken": "<jwt>",
    "user": {
      "id": "<uuid>",
      "email": "user@example.com",
      "username": "Alice",
      "isBanned": false
    }
  }
}
```

**错误码：**

| HTTP | 错误码 | 说明 |
|---|---|---|
| 401 | `AUTH_UNAUTHORIZED` | 邮箱或密码错误 |
| 403 | `FORBIDDEN` | 账号被管理员封禁 |
| 429 | `RATE_LIMITED` | 账号因连续登录失败被临时锁定 |
| 429 | `AUTH_RATE_LIMITED` | 认证端点请求过于频繁（IP 级限流） |
| 503 | `MAINTENANCE` | 系统处于维护中 |

> 连续登录失败 5 次后，账号将被锁定 15 分钟（返回 `429 RATE_LIMITED`，而非 `FORBIDDEN`）。

---

### POST /api/auth/refresh

使用 Refresh Token 换取新的 Access Token。

**认证：** 携带 Refresh Token（`Authorization: Bearer <refreshToken>` 或 Cookie `refresh_token`）

**请求体：** 空

**成功响应 200：**
```json
{
  "success": true,
  "data": {
    "accessToken": "<newJwt>",
    "user": { ... }
  }
}
```

> Refresh Token 一次性使用，刷新后旧 token 立即失效。

---

### POST /api/auth/logout

登出，撤销当前用户所有 session。

**认证：** Bearer Token（必须）

**请求体：** 空

**成功响应 200：**
```json
{
  "success": true,
  "data": { "loggedOut": true }
}
```

---

### POST /api/auth/forgot-password

发起密码重置流程（当前版本仅生成重置 token，邮件发送功能尚未启用）。

**认证：** 无

**请求体：**
```json
{
  "email": "user@example.com"
}
```

**成功响应 200：** 无论邮箱是否存在，均返回成功（防止邮箱枚举）：
```json
{
  "success": true,
  "data": {
    "emailSent": true,
    "message": "如果该邮箱已注册，将会发送密码重置链接"
  }
}
```

> 当前版本邮件发送功能未启用；服务端会创建重置 token 并写入诊断日志，响应中不会返回 token。

---

### POST /api/auth/verify-reset-token

验证密码重置 token 是否有效（不消耗 token）。

**认证：** 无

**请求体：**
```json
{
  "token": "<resetToken>"
}
```

**成功响应 200：**
```json
{
  "success": true,
  "data": { "valid": true }
}
```

---

### POST /api/auth/reset-password

使用重置 token 设置新密码。

**认证：** 无

**请求体：**
```json
{
  "token": "<resetToken>",
  "newPassword": "NewPass123"
}
```

**成功响应 200：**
```json
{
  "success": true,
  "data": {}
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `AUTH_INVALID_RESET_TOKEN` | token 无效或已使用 |
| `AUTH_EXPIRED_RESET_TOKEN` | token 已过期（有效期 1 小时） |
| `AUTH_WEAK_PASSWORD` | 新密码强度不足 |

---

## 2. 用户模块

**路径前缀：** `/api/users`
**认证：** Bearer Token（必须）

---

### GET /api/users/me

获取当前登录用户信息。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "id": "<uuid>",
    "email": "user@example.com",
    "username": "Alice",
    "isBanned": false
  }
}
```

---

### PUT /api/users/me

更新用户基本信息。

**请求体：**
```json
{
  "username": "NewName"
}
```

**响应 200：** 返回更新后的用户对象（同 GET /api/users/me）

---

### PUT /api/users/me/password

修改密码。修改后所有 session 立即失效。

**请求体：**
```json
{
  "currentPassword": "OldPass1",
  "newPassword": "NewPass1"
}
```

**响应 200：**
```json
{
  "success": true,
  "data": { "passwordChanged": true }
}
```

---

### GET /api/users/me/stats

获取用户学习统计数据。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "totalWordsLearned": 320,
    "totalSessions": 45,
    "totalRecords": 1200,
    "streakDays": 7,
    "accuracyRate": 0.82
  }
}
```

---

## 3. 用户档案

**路径前缀：** `/api/user-profile`
**认证：** Bearer Token（必须）

---

### GET /api/user-profile/reward

获取奖励偏好设置。

**响应 200：**
```json
{
  "success": true,
  "data": { "rewardType": "standard" }
}
```

`rewardType` 枚举：`standard` / `explorer` / `achiever` / `social`

---

### PUT /api/user-profile/reward

设置奖励偏好。

**请求体：**
```json
{
  "rewardType": "achiever"
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `INVALID_REWARD_TYPE` | 奖励类型不合法，必须为 standard / explorer / achiever / social 之一 |

---

### GET /api/user-profile/cognitive

获取 AMAS 自适应认知档案（处理速度、记忆容量、稳定性）。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "processingSpeed": 0.6,
    "memoryCapacity": 0.7,
    "stability": 0.5
  }
}
```

---

### GET /api/user-profile/learning-style

获取学习风格指标（同认知档案，来自 AMAS 引擎）。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "processingSpeed": 0.7,
    "memoryCapacity": 0.3,
    "stability": 0.5
  }
}
```

> 字段由 AMAS 引擎的 `cognitiveProfile` 计算得出，与 `/api/user-profile/cognitive` 返回相同字段。具体字段可能随引擎版本扩展。

---

### GET /api/user-profile/chronotype

获取用户学习时型偏好。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "chronotype": "morning",
    "preferredHours": [8, 9, 10]
  }
}
```

`chronotype` 枚举：`morning` / `evening` / `neutral`

---

### GET /api/user-profile/habit

获取用户学习习惯档案。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "preferredHours": [9, 14, 20],
    "medianSessionLengthMins": 15.0,
    "sessionsPerDay": 2.0,
    "temporalPerformance": {
      "hourlyStats": [
        {
          "sessionCount": 5,
          "avgAccuracy": 0.85,
          "avgResponseTimeMs": 1200,
          "masteryEfficiency": 0.7
        }
      ],
      "totalSessions": 45
    }
  }
}
```

> `temporalPerformance` 可能不存在：当用户已通过 `POST /api/user-profile/habit` 保存过习惯偏好，但尚未生成时段表现数据时，响应会省略该字段。

---

### POST /api/user-profile/habit

更新学习习惯偏好。

**请求体（字段均可选）：**
```json
{
  "preferredHours": [9, 14, 20],
  "medianSessionLengthMins": 20,
  "sessionsPerDay": 2
}
```

**字段约束：**
- `preferredHours`：数组元素范围 0–23
- `medianSessionLengthMins`：1–480
- `sessionsPerDay`：1–20

**错误码：**

| 错误码 | 说明 |
|---|---|
| `INVALID_PREFERRED_HOURS` | `preferredHours` 中存在超出 0–23 的小时值 |
| `INVALID_SESSIONS_PER_DAY` | `sessionsPerDay` 超出 1–20 |
| `INVALID_SESSION_LENGTH` | `medianSessionLengthMins` 超出 1–480 |

---

### POST /api/user-profile/avatar

上传用户头像。

**Content-Type：** 直接上传二进制图片（非 multipart）

**约束：**
- 最大 512 KB
- 支持格式：PNG / JPEG / GIF / WebP

**响应 200：**
```json
{
  "success": true,
  "data": { "avatarUrl": "/avatars/<userId>.png" }
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `AVATAR_EMPTY` | 未上传内容 |
| `AVATAR_TOO_LARGE` | 超过 512 KB |
| `AVATAR_INVALID_TYPE` | 格式不支持 |

---

## 4. 单词模块

**路径前缀：** `/api/words`
**认证：** Bearer Token（必须）；写操作需 Admin Token

**WordPublic 结构：**
```json
{
  "id": "<uuid>",
  "text": "abandon",
  "meaning": "放弃；遗弃",
  "pronunciation": "/əˈbændən/",
  "partOfSpeech": "verb",
  "difficulty": 0.45,
  "examples": ["He abandoned the project."],
  "tags": ["GRE", "common"],
  "createdAt": "2024-01-15T08:00:00Z"
}
```

---

### GET /api/words

分页列举单词，支持搜索。

**查询参数：**

| 参数 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `page` | number | 1 | 页码 |
| `perPage` | number | 20 | 每页条数 |
| `search` | string | — | 搜索词 |

**响应 200：** 分页结构，`data` 为 `WordPublic[]`

---

### GET /api/words/count

获取单词总数。

**响应 200：**
```json
{
  "success": true,
  "data": { "total": 5000 }
}
```

---

### GET /api/words/:id

获取单个单词。404 若不存在。

---

### POST /api/words

创建单词（Admin）。

**请求体：**
```json
{
  "id": null,
  "text": "serendipity",
  "meaning": "意外发现好事的运气",
  "pronunciation": "/ˌserənˈdɪpɪti/",
  "partOfSpeech": "noun",
  "difficulty": 0.7,
  "examples": ["A serendipitous discovery."],
  "tags": ["advanced"]
}
```

**响应 201：** WordPublic 对象

**错误码：**

| 错误码 | 说明 |
|---|---|
| `WORDS_INVALID_PAYLOAD` | `text` 或 `meaning` 为空 |

---

### PUT /api/words/:id

更新单词（Admin）。请求体字段同 POST，除 `text` 和 `meaning` 必须存在外其余均可选。`text` 或 `meaning` 传空字符串时保留原值，不更新对应字段。

---

### DELETE /api/words/:id

删除单词（Admin）。

**响应 200：**
```json
{
  "success": true,
  "data": { "deleted": true, "id": "<uuid>" }
}
```

---

### POST /api/words/batch

批量创建单词（Admin）。

**请求体：**
```json
{
  "words": [
    { "text": "apple", "meaning": "苹果", ... },
    { "text": "banana", "meaning": "香蕉", ... }
  ]
}
```

**响应 201：**
```json
{
  "success": true,
  "data": {
    "count": 2,
    "skipped": [0, 2],
    "items": [ ... ]
  }
}
```

> `skipped` 为被忽略条目的输入数组索引（从 0 开始），跳过原因：`text` 或 `meaning` 为空。

---

### POST /api/words/batch-get

批量按 ID 获取单词。

**请求体：**
```json
{
  "ids": ["<id1>", "<id2>"]
}
```

**响应 200：** `WordPublic[]`（保持请求顺序）

---

### POST /api/words/import-url

从公网 URL 导入单词列表（Admin）。

**请求体：**
```json
{
  "url": "https://example.com/wordlist.txt"
}
```

文件格式支持：
- `word\tmeaning`（制表符分隔）
- `word - meaning`（破折号分隔）
- `#` 开头为注释行

**限制：** 最大 10 MB，超时 30 秒，禁止内网/localhost 地址（SSRF 防护）

**响应 201：**
```json
{
  "success": true,
  "data": { "imported": 300, "items": [ ... ] }
}
```

---

## 5. 学习记录

**路径前缀：** `/api/records`
**认证：** Bearer Token（必须）

**LearningRecord 结构：**
```json
{
  "id": "<uuid>",
  "userId": "<uuid>",
  "wordId": "<uuid>",
  "isCorrect": true,
  "responseTimeMs": 1500,
  "sessionId": "<uuid>",
  "createdAt": "2024-01-15T08:00:00Z",
  "recordType": "all"
}
```

`recordType` 取值 `"learning"` / `"review"` / `"all"`，老数据兜底 `"all"`，写入端可选。

---

### GET /api/records

获取当前用户的学习记录（分页）。

**查询参数：** `page`、`perPage`

---

### POST /api/records

创建单条学习记录，同时触发 AMAS 自适应引擎更新。

**请求体：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `wordId` | string | ✓ | 单词 ID |
| `isCorrect` | boolean | ✓ | 答题是否正确 |
| `responseTimeMs` | number | ✓ | 响应时间（毫秒） |
| `clientRecordId` | string | — | 客户端幂等 ID，防止重复提交 |
| `sessionId` | string | — | 所属会话 ID |
| `isQuit` | boolean | — | 是否中途退出，默认 false |
| `dwellTimeMs` | number | — | 总停留时间（毫秒） |
| `pauseCount` | number | — | 暂停次数 |
| `switchCount` | number | — | 切换次数 |
| `retryCount` | number | — | 重试次数 |
| `focusLossDurationMs` | number | — | 失去焦点总时长（毫秒） |
| `interactionDensity` | number | — | 交互密度 |
| `pausedTimeMs` | number | — | 暂停总时长（毫秒） |
| `hintUsed` | boolean | — | 是否使用了提示，默认 false |
| `confusedWith` | string | — | 易混淆单词 ID，用于 IAD 混淆词对追踪 |
| `recordType` | enum | — | `"learning"` / `"review"` / `"all"`；按客户端所在 tab 标注，缺省落库 `"all"` |

**响应 201（新记录）/ 200（重复）：**
```json
{
  "success": true,
  "data": {
    "record": {
      "id": "<uuid>",
      "userId": "<uuid>",
      "wordId": "<uuid>",
      "isCorrect": true,
      "responseTimeMs": 1850,
      "sessionId": "<uuid>",
      "createdAt": "2024-01-15T08:00:00Z",
      "recordType": "learning"
    },
    "amasResult": {
      "sessionId": "<uuid>",
      "strategy": {
        "difficulty": 0.55,
        "batchSize": 8,
        "newRatio": 0.4,
        "intervalScale": 1.0,
        "reviewMode": false
      },
      "explanation": {
        "primaryReason": "user_state_balanced",
        "factors": [
          { "name": "accuracy", "value": 0.82, "impact": "positive" }
        ]
      },
      "state": { "...UserState 完整结构见 §13 GET /api/amas/state": "" },
      "wordMastery": {
        "wordId": "<uuid>",
        "memoryStrength": 0.62,
        "recallProbability": 0.78,
        "nextReviewIntervalSecs": 86400,
        "masteryLevel": "REVIEWING"
      },
      "reward": {
        "value": 0.42,
        "components": {
          "accuracyReward": 0.5,
          "speedReward": 0.1,
          "fatiguePenalty": 0.05,
          "frustrationPenalty": 0.0,
          "expectedForgetCost": 0.13
        }
      },
      "coldStartPhase": "Classify"
    },
    "duplicate": false
  }
}
```

字段说明：

| 字段 | 类型 | 说明 |
|---|---|---|
| `record` | `LearningRecord` | 见本节顶部结构 |
| `amasResult` | `ProcessResult \| null` | 重复提交时为 `null`；新记录成功时为下方结构 |
| `duplicate` | boolean | 是否为重复提交（基于 `clientRecordId` 持久去重） |

`amasResult.state` 与 `GET /api/amas/state` 完全同构（`UserState`），完整结构见 §13.1。

`amasResult.wordMastery` 是 `WordMasteryDecision \| null`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `wordId` | string | 触发的单词 ID |
| `memoryStrength` | number | 记忆强度，范围 0.0–1.0 |
| `recallProbability` | number | 当前回忆概率，范围 0.0–1.0 |
| `nextReviewIntervalSecs` | number | 下次复习间隔（秒），用于回填 `WordLearningState.nextReviewDate` |
| `masteryLevel` | enum | `"NEW" / "LEARNING" / "REVIEWING" / "MASTERED" / "FORGOTTEN"`，**SCREAMING_SNAKE_CASE**；注意与 `WordLearningState.state`（lowercase）是**不同枚举**，不可混用 |

`amasResult.coldStartPhase` 是 `"Classify" | "Explore" | null`（**PascalCase**，不带 rename）；`null` 表示已进入稳定期。

> **幂等行为**：服务端基于 `(userId, clientRecordId)` 持久去重，任意时刻再次提交相同 `clientRecordId` 都会返回已有记录（`duplicate: true`），**无 5 秒窗口限制**。若未指定 `clientRecordId`，每次调用都会写入新记录。

---

### POST /api/records/batch

批量创建学习记录。

**请求体：**
```json
{
  "records": [ { ...同 POST /api/records 的请求体... } ]
}
```

**响应：**
- `201 Created`：全部写入成功（`partial: false`）
- `200 OK`：部分失败（`partial: true`）

```json
{
  "success": true,
  "data": {
    "count": 10,
    "failed": 0,
    "partial": false,
    "items": [ ... ],
    "errors": []
  }
}
```

`errors` 数组元素结构：

| 字段 | 类型 | 说明 |
|---|---|---|
| `index` | number | 失败条目在 `records` 入参数组中的位置（从 0 开始） |
| `code` | string | 业务错误码 |
| `message` | string | 错误描述 |

> 同一 `clientRecordId` 的重复提交在 `items` 中表现为 `duplicate: true`，不计入 `failed`。

---

### GET /api/records/statistics

获取基础统计。

**查询参数：**

| 参数 | 类型 | 说明 |
|---|---|---|
| `category` | enum | 可选，`"learning"` / `"review"` / `"all"`（缺省 `all` 等同不过滤），仅过滤 `total` / `correct` / `accuracy` |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "total": 1200,
    "correct": 984,
    "accuracy": 0.82,
    "totalDurationSecs": 32400
  }
}
```

> `totalDurationSecs` 来自 `learning_sessions` 全量 SUM，**不随 `category` 变化**——反映该用户累计学习时长。

---

### GET /api/records/statistics/enhanced

获取包含每日明细和连续天数的增强统计。

**查询参数：**

| 参数 | 类型 | 说明 |
|---|---|---|
| `category` | enum | 可选，`"learning"` / `"review"` / `"all"`，过滤 `total` / `correct` / `accuracy` 与 `daily[].total/correct/accuracy/newWords` |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "total": 1200,
    "correct": 984,
    "accuracy": 0.82,
    "streak": 7,
    "totalDurationSecs": 32400,
    "daily": [
      {
        "date": "2024-01-15",
        "total": 50,
        "correct": 42,
        "accuracy": 0.84,
        "newWords": 12,
        "durationSecs": 1800
      }
    ]
  }
}
```

> `daily[]` 仅包含**有记录**的日期；`durationSecs` 为该日期对应 `learning_sessions` 的时长之和（DB GROUP BY 聚合，不受 `max_stats_records` 上限影响）。`newWords` 为该日"首次出现"的单词数。`streak` 严格基于 `daily[].date` 集合，session-only 日期不参与。`totalDurationSecs` 与 `/records/statistics` 一致，不随 `category` 变化。

---

## 6. 学习会话

**路径前缀：** `/api/learning`
**认证：** Bearer Token（必须）

---

### POST /api/learning/session

创建或恢复学习会话。

**请求体：**
```json
{
  "targetMasteryCount": 10
}
```

`targetMasteryCount`：可选，本次会话目标掌握词数；若不传则取学习配置中的 `dailyMasteryTarget`。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "sessionId": "<uuid>",
    "status": "active",
    "resumed": false,
    "targetMasteryCount": 10,
    "crossSessionHint": {
      "prevAccuracy": 0.8,
      "prevMasteredCount": 8,
      "gapMinutes": 120,
      "suggestedDifficulty": 0.55,
      "errorProneWordIds": ["<id1>"],
      "recentlyMasteredWordIds": ["<id2>"]
    }
  }
}
```

> 若已有进行中的会话，`resumed=true` 并返回已有 `sessionId`；`crossSessionHint` 仅在上次会话结束于 2 小时内时出现。

---

### GET /api/learning/study-words

获取当前会话的学习单词集（由 AMAS 自适应算法筛选）。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "words": [ ...WordPublic... ],
    "strategy": {
      "difficultyRange": [0.3, 0.7],
      "newRatio": 0.4,
      "batchSize": 10
    }
  }
}
```

---

### POST /api/learning/next-words

根据当前会话进度动态获取下一批单词。

**请求体：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `excludeWordIds` | string[] | ✓ | 当前轮次已显示的单词 ID，排除重复 |
| `masteredWordIds` | string[] | — | 本轮已掌握的单词 ID，触发状态更新 |
| `sessionPerformance` | object | — | 本次会话实时表现，用于策略动态调整 |

`sessionPerformance` 结构：
```json
{
  "recentAccuracy": 0.75,
  "masteredCount": 5,
  "targetMasteryCount": 10,
  "errorProneWordIds": ["<id1>"]
}
```

**响应 200：**
```json
{
  "success": true,
  "data": {
    "words": [ ...WordPublic... ],
    "batchSize": 8
  }
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `LEARNING_TOO_MANY_EXCLUDES` | `excludeWordIds` 数量超过限制值 |

---

### POST /api/learning/adjust-words

根据用户状态调整学习策略。

**请求体：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `recentPerformance` | number (0–1) | 最近几题的准确率 |
| `userState` | string | 用户状态枚举（见下） |

`userState` 枚举：

| 值 | 效果 |
|---|---|
| `focused` / `engaged` / `confident` | 提高难度和新词比例 |
| `tired` / `fatigued` / `frustrated` / `distracted` | 降低难度、减少批次大小 |
| `review` | 切换为复习模式，新词比例为 0 |
| `sprint` | 冲刺模式，新词比例最大化 |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "adjustedStrategy": {
      "difficulty": 0.6,
      "batchSize": 8,
      "newRatio": 0.5,
      "intervalScale": 1.0,
      "reviewMode": false
    }
  }
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `LEARNING_INVALID_RECENT_PERFORMANCE` | `recentPerformance` 不是 0–1 之间的有限数值 |

---

### POST /api/learning/sync-progress

同步会话进度指标到服务端。

**请求体：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `sessionId` | string | ✓ | 会话 ID |
| `totalQuestions` | number | — | 总答题数 |
| `contextShifts` | number | — | 上下文切换次数 |

**响应 200：** `{success, data}` 信封，`data` 为更新后的 LearningSession 对象（结构见下方 complete-session 示例；同步进度场景 `summary` 通常为 `null`）。

---

### POST /api/learning/complete-session

完成并关闭当前学习会话。

**请求体：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `sessionId` | string | ✓ | 会话 ID |
| `masteredWordIds` | string[] | ✓ | 本次已掌握的单词 ID 列表 |
| `errorProneWordIds` | string[] | ✓ | 本次易错单词 ID 列表 |
| `avgResponseTimeMs` | number | ✓ | 平均响应时间（毫秒） |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "id": "<uuid>",
    "userId": "<uuid>",
    "status": "completed",
    "targetMasteryCount": 10,
    "totalQuestions": 30,
    "actualMasteryCount": 8,
    "contextShifts": 2,
    "createdAt": "<ISO-8601>",
    "updatedAt": "<ISO-8601>",
    "summary": {
      "accuracy": 0.85,
      "avgResponseTimeMs": 1200,
      "masteredWordIds": ["<id1>"],
      "errorProneWordIds": ["<id2>"],
      "durationSecs": 600,
      "hourOfDay": 14,
      "finalDifficulty": 0.6
    },
    "correctCount": 0,
    "totalCount": 0
  }
}
```

> 注意：`data` 是 LearningSession 对象本身，主键字段是 `id`（不是 `sessionId`，与 `POST /api/learning/session` 返回的 `SessionResponse` 不同）。complete-session 写入后 `summary` 必为对象；sync-progress 在会话尚未完成时 `summary` 为 `null`。

---

### POST /api/learning/pick-next-word

从活跃单词集中选取下一个要显示的单词（考虑错词优先策略）。

**请求体：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `activeWordIds` | string[] | ✓ | 当前活跃单词集（非空） |
| `errorWordIds` | string[] | ✓ | 易错单词集 |
| `lastShownMap` | object | — | `{wordId: timestamp}` 各词上次展示时间 |
| `priorityMap` | object | — | `{wordId: priority}` 各词优先级 |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "word": { ...WordPublic... },
    "priority": "error_review"
  }
}
```

`priority`：`"error_review"` 或 `"normal"`

**错误码：**

| 错误码 | 说明 |
|---|---|
| `LEARNING_NO_ACTIVE_WORDS` | `activeWordIds` 为空 |
| `LEARNING_TOO_MANY_IDS` | `activeWordIds` 数量超限 |

---

### POST /api/learning/generate-options

为指定单词生成多选题选项（1 个正确 + 3 个干扰项）。

**请求体：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `wordId` | string | ✓ | 目标单词 ID |
| `mode` | string | ✓ | `"word-to-meaning"` 或 `"meaning-to-word"` |
| `poolWordIds` | string[] | ✓ | 干扰项候选单词池 |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "options": ["放弃", "探索", "坚持", "逃避"],
    "correctIndex": 0
  }
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `LEARNING_INVALID_MODE` | `mode` 仅支持 `word-to-meaning` 或 `meaning-to-word` |
| `LEARNING_TOO_MANY_IDS` | `poolWordIds` 数量超限 |

---

## 7. 单词学习状态

**路径前缀：** `/api/word-states`
**认证：** Bearer Token（必须）

**WordLearningState 结构：**
```json
{
  "userId": "<uuid>",
  "wordId": "<uuid>",
  "state": "mastered",
  "masteryLevel": 0.95,
  "nextReviewDate": "2024-01-20T08:00:00Z",
  "halfLife": 72.0,
  "correctStreak": 5,
  "totalAttempts": 12,
  "updatedAt": "2024-01-15T08:00:00Z",
  "bookmarked": false
}
```

> `bookmarked` 为只读字段，由后端 join `word_favorites` 批量回填；写收藏走 `/api/word-favorites/:wordId`（见第 19 节），通过该字段做客户端星标回填，不必再调 `/api/word-favorites/status`。本节所有读侧端点（`GET /:wordId`、`POST /batch`、`GET /due/list`、`POST /:wordId/mark-mastered`、`POST /:wordId/reset`）返回的 `WordLearningState` 都带 `bookmarked`。

---

### GET /api/word-states/:wordId

获取某单词的学习状态。404 若尚无记录。

---

### POST /api/word-states/batch

批量查询学习状态。

**请求体：**
```json
{
  "wordIds": ["<id1>", "<id2>"]
}
```

**响应 200：** `WordLearningState[]`（仅返回已有记录的条目）

---

### GET /api/word-states/due/list

获取到期需复习的单词。

**查询参数：** `limit`（默认 50，范围 1–200）

**响应 200：** `WordLearningState[]`

---

### GET /api/word-states/stats/overview

获取各学习状态分布统计。

**查询参数：**

| 参数 | 类型 | 说明 |
|---|---|---|
| `category` | enum | 可选，`"learning"` / `"review"` / `"all"`；传 `learning` / `review` 时通过 `learning_records` 子查询限定 word 集合（即"该用户在该 category 下出现过的 word 的当前 state 分布"），缺省/`all` 等同不过滤 |

**错误码：**

| 错误码 | 说明 |
|---|---|
| `INVALID_CATEGORY` | `category` 不在 `learning` / `review` / `all` 范围内 |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "newCount": 120,
    "learning": 85,
    "reviewing": 60,
    "mastered": 200,
    "forgotten": 15
  }
}
```

---

### POST /api/word-states/:wordId/mark-mastered

将指定单词标记为已掌握（masteryLevel=1.0）。若记录不存在则自动创建。

---

### POST /api/word-states/:wordId/reset

重置指定单词的学习进度（回到 NEW 状态）。

---

### POST /api/word-states/batch-update

批量更新学习状态。

**请求体：**
```json
{
  "updates": [
    {
      "wordId": "<id>",
      "state": "reviewing",
      "masteryLevel": 0.6
    }
  ]
}
```

`state` 和 `masteryLevel` 均为可选；所有 `wordId` 必须已存在。`state` 枚举值为 lowercase（`"new"/"learning"/"reviewing"/"mastered"/"forgotten"`，v0.6.0-beta.4 P3#7 起）。

**响应 200：**
```json
{
  "success": true,
  "data": { "updated": 3 }
}
```

---

## 8. 单词本

**路径前缀：** `/api/wordbooks`
**认证：** Bearer Token（必须）

**Wordbook 结构：**
```json
{
  "id": "<uuid>",
  "name": "GRE 核心词汇",
  "description": "3000 个 GRE 高频词",
  "type": "system",
  "userId": null,
  "wordCount": 3000,
  "createdAt": "2024-01-01T00:00:00Z"
}
```

---

### GET /api/wordbooks/system

获取所有系统单词本列表。

---

### GET /api/wordbooks/user

获取当前用户创建的自定义单词本列表。

---

### POST /api/wordbooks

创建自定义单词本。

**请求体：**
```json
{
  "name": "我的错题本",
  "description": "收集做错的单词"
}
```

**响应 201：** Wordbook 对象

---

### GET /api/wordbooks/:id/words

获取指定单词本内的单词（分页）。用户只能访问自己的或系统单词本。

**查询参数：** `page`、`perPage`

---

### POST /api/wordbooks/:id/words

向单词本添加单词（仅限自定义单词本所有者）。

**请求体：**
```json
{
  "wordIds": ["<id1>", "<id2>"]
}
```

**响应 200：**
```json
{
  "success": true,
  "data": { "added": 2 }
}
```

---

### DELETE /api/wordbooks/:id/words/:wordId

从单词本移除单词。

**响应 200：**
```json
{
  "success": true,
  "data": { "removed": true }
}
```

---

## 9. 单词本中心

**路径前缀：** `/api/wordbook-center`
**认证：** Bearer Token（必须）

单词本中心允许用户从配置的远程 URL 浏览并导入词库。

---

### GET /api/wordbook-center/settings

获取用户的单词本中心 URL 配置。

**响应 200：**
```json
{
  "success": true,
  "data": { "wordbookCenterUrl": "https://..." }
}
```

---

### PUT /api/wordbook-center/settings

设置单词本中心 URL。

**请求体：**
```json
{
  "wordbookCenterUrl": "https://cdn.example.com/wordbook-center"
}
```

`wordbookCenterUrl`：公网 HTTP/HTTPS URL；传 `null` 可清空配置。

---

### GET /api/wordbook-center/browse

浏览远程词库列表。

**响应 200：**
```json
{
  "success": true,
  "data": [
    {
      "id": "gre-core",
      "name": "GRE 核心词汇",
      "description": "...",
      "wordCount": 3000,
      "coverImage": null,
      "tags": ["GRE"],
      "version": "1.2.0",
      "author": "WordForge",
      "downloadCount": 5000,
      "imported": false,
      "localWordbookId": null,
      "localVersion": null,
      "hasUpdate": false
    }
  ]
}
```

---

### GET /api/wordbook-center/browse/:id

预览远程词库内容（分页）。

**查询参数：** `page`、`perPage`

---

### POST /api/wordbook-center/import/:id

从单词本中心导入词库为用户自定义单词本。

**请求体：** 空

**响应 201：**
```json
{
  "success": true,
  "data": {
    "wordbook": { ...Wordbook... },
    "wordsImported": 3000,
    "wordsSkipped": 0
  }
}
```

---

### POST /api/wordbook-center/import-url

从任意公网 URL 导入词库。SSRF 防护规则同 `/api/words/import-url`。

**请求体：**
```json
{
  "url": "https://example.com/my-wordbook.json"
}
```

---

### GET /api/wordbook-center/updates

获取用户已导入词库的可用更新列表。

**响应 200：**
```json
{
  "success": true,
  "data": [
    {
      "remoteId": "gre-core",
      "name": "GRE 核心词汇",
      "localVersion": "1.1.0",
      "remoteVersion": "1.2.0",
      "localWordbookId": "<uuid>"
    }
  ]
}
```

---

### POST /api/wordbook-center/updates/:id/sync

同步指定词库到最新版本。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordbook": { ... },
    "wordsAdded": 50,
    "wordsUpdated": 10,
    "wordsRemoved": 5
  }
}
```

---

## 10. 学习配置

**路径前缀：** `/api/study-config`
**认证：** Bearer Token（必须）

---

### GET /api/study-config

获取学习配置。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "userId": "<uuid>",
    "selectedWordbookIds": ["<id1>"],
    "dailyWordCount": 20,
    "studyMode": "normal",
    "dailyMasteryTarget": 10
  }
}
```

`studyMode` 枚举：`normal` / `intensive` / `review` / `casual`

---

### PUT /api/study-config

更新学习配置（字段均可选）。

**请求体：**
```json
{
  "selectedWordbookIds": ["<id1>", "<id2>"],
  "dailyWordCount": 30,
  "studyMode": "intensive",
  "dailyMasteryTarget": 15
}
```

**字段约束：**
- `dailyWordCount`：1–200
- `dailyMasteryTarget`：1–100
- `selectedWordbookIds`：需为已存在的单词本 ID（不限系统或自定义）

**错误码：**

| 错误码 | 说明 |
|---|---|
| `WORDBOOK_NOT_FOUND` | `selectedWordbookIds` 中包含不存在的单词本 ID |

---

### GET /api/study-config/today-words

获取今日待学单词。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "words": [ ...Word... ],
    "target": 20
  }
}
```

> 当前实现返回内部 `Word` 结构，包含 `embedding` 字段（值可能为 `null` 或 number 数组）。普通客户端展示时可忽略该字段。

---

### GET /api/study-config/progress

获取今日学习进度。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "studied": 8,
    "target": 15,
    "new": 3,
    "learning": 5,
    "reviewing": 4,
    "mastered": 8
  }
}
```

---

## 11. 通知与偏好

**路径前缀：** `/api/notifications`
**认证：** Bearer Token（必须）

---

### GET /api/notifications

获取通知列表。

**查询参数：**

| 参数 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `limit` | number | 50 | 最多返回数量，范围 1–200 |
| `unreadOnly` | boolean | false | 仅返回未读 |

**响应 200：**
```json
{
  "success": true,
  "data": [
    {
      "id": "<uuid>",
      "userId": "<uuid>",
      "type": "reminder",
      "title": "复习提醒",
      "message": "有单词需要复习",
      "wordId": "<uuid>",
      "overdueHours": 3,
      "read": false,
      "createdAt": "2024-01-15T08:00:00Z"
    }
  ]
}
```

`type` 枚举：`system` / `achievement` / `reminder` / `info` / `broadcast`。`wordId`、`overdueHours` 为可选字段，无值时不返回。

---

### GET /api/notifications/unread-count

获取未读通知数量。

**响应 200：**
```json
{
  "success": true,
  "data": { "unreadCount": 3 }
}
```

---

### PUT /api/notifications/:id/read

将指定通知标为已读。

---

### POST /api/notifications/read-all

将所有通知标为已读。

**响应 200：**
```json
{
  "success": true,
  "data": { "markedRead": 5 }
}
```

---

### GET /api/notifications/badges

获取徽章列表及进度。

**响应 200：**
```json
{
  "success": true,
  "data": [
    {
      "id": "first_word",
      "name": "First Word",
      "description": "Learn your first word",
      "unlocked": true,
      "progress": 1.0,
      "unlockedAt": "2024-01-15T08:00:00Z"
    },
    {
      "id": "streak_7",
      "name": "Week Streak",
      "description": "Study for 7 consecutive days",
      "unlocked": false,
      "progress": 0.43,
      "unlockedAt": null
    },
    {
      "id": "mastered_100",
      "name": "Century Club",
      "description": "Master 100 words",
      "unlocked": false,
      "progress": 0.32,
      "unlockedAt": null
    }
  ]
}
```

---

### GET /api/notifications/preferences

获取用户 UI 偏好。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "theme": "light",
    "language": "en",
    "notificationEnabled": true,
    "soundEnabled": true
  }
}
```

---

### PUT /api/notifications/preferences

更新 UI 偏好（字段均可选）。

**请求体：**
```json
{
  "theme": "dark",
  "language": "zh",
  "notificationEnabled": true,
  "soundEnabled": false
}
```

**字段约束：**
- `theme`：`light` / `dark` / `system`
- `language`：`en` / `zh` / `ja` / `ko` / `fr` / `de` / `es`

---

## 12. 内容增强

**路径前缀：** `/api/content`
**认证：** Bearer Token（必须）；写操作（`POST /api/content/morphemes/:wordId`）需 Admin Token

---

### GET /api/content/etymology/:wordId

获取单词词源解析。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordId": "<uuid>",
    "word": "abandon",
    "etymology": "源自古法语 abandoner，含义为...",
    "roots": ["a-", "bandon"],
    "generated": false,
    "source": "rule_based_fallback"
  }
}
```

> `generated` 为 `true` 表示由 LLM 生成；`false` 表示由规则引擎回退生成。`source` 可能为 `"llm"` 或 `"rule_based_fallback"`。

---

### GET /api/content/semantic/search

语义搜索单词（当前为关键词回退实现）。

**查询参数：**

| 参数 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `query` | string | ✓ | 搜索词 |
| `limit` | number | — | 返回数量，默认 10，最大 50 |

**响应 200：**
```json
{
  "success": true,
  "data": {
    "query": "forgiveness",
    "results": [ ...WordPublic... ],
    "total": 5,
    "method": "keyword_fallback",
    "degraded": true
  }
}
```

---

### GET /api/content/word-contexts/:wordId

获取单词的使用语境和例句。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordId": "<uuid>",
    "word": "abandon",
    "examples": ["He abandoned the project."],
    "contexts": [
      {
        "id": "<wordId>-ctx-0",
        "sentence": "He abandoned the project.",
        "source": "word_examples"
      }
    ]
  }
}
```

---

### GET /api/content/morphemes/:wordId

获取单词词素拆解。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordId": "<uuid>",
    "morphemes": [
      { "text": "un-", "type": "prefix", "meaning": "不、非" },
      { "text": "forgiv", "type": "root", "meaning": "宽恕" },
      { "text": "-able", "type": "suffix", "meaning": "能够…的" }
    ]
  }
}
```

> `type` 是服务端按字符串保存和返回的字段；推荐使用 `"prefix"` / `"root"` / `"suffix"`，但当前实现不做枚举校验。

---

### GET /api/content/confusion-pairs/:wordId

获取易混淆单词（按相似度排序）。

**查询参数：** `limit`（默认 20，范围 1–100）

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordId": "<uuid>",
    "confusionPairs": [
      {
        "wordId": "<otherId>",
        "word": "affect",
        "meaning": "影响",
        "similarity": 0.87
      }
    ]
  }
}
```

---

## 13. AMAS 自适应引擎

**路径前缀：** `/api/amas`
**认证：** Bearer Token（必须）

AMAS（Adaptive Multi-level Adjustment System）是服务端自适应学习引擎，负责根据用户答题行为计算用户状态（注意力、疲劳、动机、自信）、生成学习策略、评估单词掌握度等。本组接口面向普通用户，返回内容均属于当前登录用户自身的状态。

---

### GET /api/amas/state

获取当前用户的 AMAS 状态向量。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "attention": 0.72,
    "fatigue": 0.28,
    "motivation": 0.65,
    "confidence": 0.54,
    "lastActiveAt": "2024-01-15T08:00:00Z",
    "sessionEventCount": 12,
    "totalEventCount": 834,
    "createdAt": "2024-01-01T00:00:00Z",
    "cognitiveProfile": {
      "memoryCapacity": 0.5,
      "processingSpeed": 0.5,
      "stability": 0.5
    },
    "trendState": {
      "accuracyTrend": 0.0,
      "speedTrend": 0.0,
      "engagementTrend": 0.0
    },
    "habitProfile": {
      "preferredHours": [9, 14, 20],
      "medianSessionLengthMins": 15.0,
      "sessionsPerDay": 1.0,
      "temporalPerformance": {
        "hourlyStats": [
          { "sessionCount": 5, "avgAccuracy": 0.85, "avgResponseTimeMs": 1200, "masteryEfficiency": 0.7 }
        ],
        "totalSessions": 45
      }
    },
    "lastSessionId": null
  }
}
```

> `cognitiveProfile.memoryCapacity`、`processingSpeed`、`stability` 范围均为 0.0–1.0。
> `trendState` 各趋势值正值表示上升，负值表示下降。
> `habitProfile.preferredHours` ≤ 3 个 0–23 的小时值，`temporalPerformance.hourlyStats` 在冷启动期返回 24 个全零条目（每小时一条），稳定期后填充真实数据。
> `lastSessionId` 为 `null` 时表示尚无活跃会话。

---

### GET /api/amas/strategy

根据当前用户状态计算推荐的学习策略参数。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "difficulty": 0.55,
    "batchSize": 8,
    "newRatio": 0.4,
    "intervalScale": 1.0,
    "reviewMode": false
  }
}
```

---

### GET /api/amas/phase

获取当前用户所处的冷启动阶段。

**响应 200：**
```json
{
  "success": true,
  "data": { "phase": "Classify" }
}
```

`phase` 可能值：`"Classify"`（分类期）/ `"Explore"`（探索期）/ `null`（已进入稳定期）

---

### GET /api/amas/learning-curve

获取按日聚合的学习曲线（最近 1000 条记录）。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "curve": [
      { "date": "2024-01-13", "total": 50, "correct": 42, "accuracy": 0.84 },
      { "date": "2024-01-14", "total": 60, "correct": 54, "accuracy": 0.90 }
    ]
  }
}
```

---

### GET /api/amas/retention-curve

获取按"距首次学习天数"聚合的保持率曲线（艾宾浩斯遗忘曲线）。

**桶分配规则：**
- 桶为 `[1, 2, 4, 7, 15, 30]` 天（固定 6 个点）
- 每个 word 按 `(now - 首次记录时间)` 的天数最近邻匹配到一个桶
- 距首次学习不足 0.5 天的样本不计入（防止刚学的词污染数据）
- retention 复用指数衰减 / MDM 兜底（与 `/api/analytics/forgetting-risk` 同公式），仅分桶维度不同

**响应 200：**
```json
{
  "success": true,
  "data": {
    "points": [
      { "daysSinceLearn": 1,  "retention": 0.92, "sampleSize": 47 },
      { "daysSinceLearn": 2,  "retention": 0.85, "sampleSize": 38 },
      { "daysSinceLearn": 4,  "retention": 0.71, "sampleSize": 25 },
      { "daysSinceLearn": 7,  "retention": 0.58, "sampleSize": 18 },
      { "daysSinceLearn": 15, "retention": null, "sampleSize": 0 },
      { "daysSinceLearn": 30, "retention": null, "sampleSize": 0 }
    ],
    "averageRetention": 0.78
  }
}
```

> `points` 总返回 6 个桶且固定按 `daysSinceLearn` 升序，UI 不需要排序或补桶。某桶 `sampleSize=0` 时 `retention` 为 `null`（前端按空状态绘制）。`averageRetention` 是全部样本的算术平均，无样本时为 `null`。

---

### GET /api/amas/intervention

获取针对当前用户状态的干预建议（疲劳提醒、专注建议、鼓励等）。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "interventions": [
      {
        "type": "rest",
        "message": "您似乎有些疲劳，建议休息一下",
        "severity": "warning"
      }
    ]
  }
}
```

`type`：`"rest"`（疲劳过高）/ `"encouragement"`（动机过低）/ `"focus"`（注意力下降）/ `"continue"`（表现良好）
`severity`：`"warning"` / `"info"` / `"success"`

---

### GET /api/amas/mastery/evaluate

评估指定单词的掌握度。

**查询参数：** `wordId`（必填）

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordId": "<uuid>",
    "state": "REVIEWING",
    "masteryLevel": 0.72,
    "correctStreak": 3,
    "totalAttempts": 8,
    "nextReviewDate": "2024-01-20T08:00:00Z"
  }
}
```

> 若用户对该单词尚无学习记录，返回 `state: "NEW"`、`masteryLevel: 0.0`、`correctStreak: 0`、`totalAttempts: 0`、`nextReviewDate: null`。

---

### POST /api/amas/process-event

上报一条学习事件并触发 AMAS 状态更新（不写入 `learning_records` 表）。通常由底层 SDK 使用；应用层应优先调用 `POST /api/records`。

**请求体：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `wordId` | string | ✓ | 单词 ID |
| `isCorrect` | boolean | ✓ | 是否答对 |
| `responseTime` | number | ✓ | 响应时间（毫秒），亦可使用 `response_time` snake_case |
| `sessionId` | string | — | 所属会话 ID |
| `isQuit` | boolean | — | 是否中途退出，默认 false |
| `dwellTime` | number | — | 停留时长（毫秒） |
| `pauseCount` | number | — | 暂停次数 |
| `switchCount` | number | — | 切换次数 |
| `retryCount` | number | — | 重试次数 |
| `focusLossDuration` | number | — | 失去焦点累计时长（毫秒） |
| `interactionDensity` | number | — | 交互密度 |
| `pausedTimeMs` | number | — | 暂停累计时长（毫秒） |
| `hintUsed` | boolean | — | 是否使用提示，默认 false |
| `confusedWith` | string | — | 易混淆单词 ID，用于 IAD 混淆词对追踪 |

**响应 200：** `ProcessResult` 对象。

```json
{
  "success": true,
  "data": {
    "sessionId": "<uuid>",
    "strategy": {
      "difficulty": 0.55,
      "batchSize": 8,
      "newRatio": 0.4,
      "intervalScale": 1.0,
      "reviewMode": false
    },
    "explanation": {
      "primaryReason": "user_state_balanced",
      "factors": [
        { "name": "accuracy", "value": 0.82, "impact": "positive" }
      ]
    },
    "state": {
      "attention": 0.72,
      "fatigue": 0.28,
      "motivation": 0.65,
      "confidence": 0.54,
      "lastActiveAt": "2024-01-15T08:00:00Z",
      "sessionEventCount": 12,
      "totalEventCount": 834,
      "createdAt": "2024-01-01T00:00:00Z",
      "cognitiveProfile": { "memoryCapacity": 0.5, "processingSpeed": 0.5, "stability": 0.5 },
      "trendState": { "accuracyTrend": 0.0, "speedTrend": 0.0, "engagementTrend": 0.0 },
      "habitProfile": {
        "preferredHours": [9, 14, 20],
        "medianSessionLengthMins": 15.0,
        "sessionsPerDay": 1.0,
        "temporalPerformance": {
          "hourlyStats": [
            { "sessionCount": 5, "avgAccuracy": 0.85, "avgResponseTimeMs": 1200, "masteryEfficiency": 0.7 }
          ],
          "totalSessions": 45
        }
      },
      "lastSessionId": null
    },
    "wordMastery": {
      "wordId": "<uuid>",
      "memoryStrength": 0.62,
      "recallProbability": 0.78,
      "nextReviewIntervalSecs": 86400,
      "masteryLevel": "REVIEWING"
    },
    "reward": {
      "value": 0.42,
      "components": {
        "accuracyReward": 0.5,
        "speedReward": 0.1,
        "fatiguePenalty": 0.05,
        "frustrationPenalty": 0.0,
        "expectedForgetCost": 0.13
      }
    },
    "coldStartPhase": "Classify"
  }
}
```

`ProcessResult` 字段一览：

| 字段 | 类型 | 说明 |
|---|---|---|
| `sessionId` | string | 当前活跃会话 ID |
| `strategy` | `StrategyParams` | 见 `GET /api/amas/strategy` |
| `explanation` | object | `{ primaryReason: string, factors: [{ name, value, impact }] }`；`impact` 常见值 `"positive" / "negative" / "neutral"`，服务端不做枚举校验 |
| `state` | `UserState` | 与 `GET /api/amas/state` 完全同构 |
| `wordMastery` | `WordMasteryDecision \| null` | 见下表；少数路径（如部分早退场景）为 `null` |
| `reward` | object | `{ value: f64, components: { accuracyReward, speedReward, fatiguePenalty, frustrationPenalty, expectedForgetCost } }` |
| `coldStartPhase` | `"Classify" \| "Explore" \| null` | **PascalCase 不带 rename**；`null` 表示已进入稳定期 |

`wordMastery` 字段：

| 字段 | 类型 | 说明 |
|---|---|---|
| `wordId` | string | 触发的单词 ID |
| `memoryStrength` | number | 记忆强度，范围 0.0–1.0 |
| `recallProbability` | number | 当前回忆概率，范围 0.0–1.0 |
| `nextReviewIntervalSecs` | number | 下次复习间隔（秒），用于回填 `WordLearningState.nextReviewDate` |
| `masteryLevel` | enum | `"NEW" / "LEARNING" / "REVIEWING" / "MASTERED" / "FORGOTTEN"`，**SCREAMING_SNAKE_CASE**；注意与 `WordLearningState.state`（lowercase）是**不同枚举**，不可混用 |

> `POST /api/records` 响应中的 `data.amasResult` 与本端点返回的 `data` 同构；区别仅在于 `/api/records` 同时把记录写入 `learning_records` 表并触发 ELO/word-state 持久化。

---

### POST /api/amas/batch-process

批量上报学习事件。

**请求体：**
```json
{
  "events": [ { ...同 process-event 请求体... } ]
}
```

**约束：** `events` 长度上限受 `max_batch_size` 限制（默认 500），超限返回 `BATCH_TOO_LARGE`。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "count": 3,
    "items": [ ...ProcessResult[]... ]
  }
}
```

---

### POST /api/amas/reset

重置当前用户的 AMAS 状态（恢复到默认值）。

**请求体：** 空

**响应 200：**
```json
{
  "success": true,
  "data": { "reset": true }
}
```

---

### POST /api/amas/visual-fatigue

上报客户端计算的视觉疲劳分数，触发 AMAS 状态更新。

**请求体：**

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `score` | number | ✓ | 视觉疲劳分数，范围 0–100 |

**响应 200：** 更新后的 AMAS `UserState` 对象（同 `GET /api/amas/state`）

**错误码：**

| 错误码 | 说明 |
|---|---|
| `INVALID_SCORE` | 分数不在 0–100 范围内 |

---

## 14. 实时推送

**路径前缀：** `/api/realtime`
**认证：** Bearer Token（必须）

---

### GET /api/realtime/events

Server-Sent Events（SSE）持久连接，用于接收服务端实时推送。

**可选请求头：**
- `x-device-id`：设备 ID
- `x-device-platform`：设备平台

**事件类型：**

| 事件名 (`event:`) | 说明 | `data` JSON 结构 |
|---|---|---|
| `amas_state` | AMAS 引擎状态变化（服务端每 5 秒轮询，仅当 `totalEventCount` 增长时推送） | `{"type": "state_change", "attention": 0.72, "fatigue": 0.28, "motivation": 0.65, "confidence": 0.54, "sessionEventCount": 12, "totalEventCount": 834}` |
| `maintenance` | 维护模式变化 | `{"type": "maintenance", "active": true}` |
| `update_available` | 新版本可用（面向所有用户的刷新提示，由 `broadcast_update` 发出；与管理员专属的 `release_available` 不同） | `{"version": "v1.1.1", "message": "新版本已发布"}` |
| `telemetry_request` | 服务器请求遥测上报 | `{"type": "telemetry_request", "requestId": "<uuid>"}` |
| `banned` | 账号被封禁 | `{"type": "banned"}` |
| `unbanned` | 账号解封 | `{"type": "unbanned"}` |
| `data_corrupted` | 数据异常通知 | `{"type": "data_corrupted"}` |
| `new_llm_suggestion` | 新的 LLM 调参建议到达（管理员专属，advisor 页可立即刷新） | `{"type": "new_llm_suggestion", "suggestionId": 42}` |
| `release_available` | 自更新 worker 探测到 GitHub Releases 有新二进制；v0.6.0-beta.3 起含 `channel`（管理员专属） | `{"type": "release_available", "latestTag": "v1.1.1", "channel": "stable"}` |
| `update_progress` | 一键更新执行阶段进度（管理员专属，0–100） | `{"type": "update_progress", "phase": "downloading", "percent": 35}` |
| `probe_request` | 远程探针脚本下发：admin 通过 `POST /api/admin/probe` 派发，客户端在 Worker 沙箱 eval 后回传结果 | `{"type": "probe_request", "requestId": "<uuid>", "batchId": "<uuid>", "scriptB64": "...", "timeoutMs": 3000, "ctxVersion": 1}` |
| `probe_confirm` | 远程探针二次确认：客户端首次返回 `confirm_required` 后，admin 输入设备后 5 位确认，后端推此事件 | `{"type": "probe_confirm", "requestId": "<uuid>", "confirmToken": "<uuid>"}` |
| `incident` | 5xx 错误率超阈值告警（管理员专属，5 分钟窗口、同窗口 dedup） | `{"type": "incident", "errorRate": 0.025, "windowSecs": 300}` |
| `worker_missed` | 调度器健康告警：worker 连续 3 个周期未上报（管理员专属） | `{"type": "worker_missed", "workerName": "amas_aggregator", "missCount": 3}` |
| `llm_budget_exceeded` | LLM advisor 月度成本超限，当月 worker 已停跑（管理员专属） | `{"type": "llm_budget_exceeded", "spentYuan": 102.5, "capYuan": 100.0, "resumeMonth": "2026-06"}` |
| `resource_pack_available` | v1.1：admin 切换 channel 激活资源包后广播，客户端拉 manifest 并下载安装（5 分钟内同 pack 不重复推送） | `{"type": "resource_pack_available", "packId": "wordbook-core", "version": "1.2.3", "channel": "stable"}` |

> 服务端 SSE Keep-alive 文本行 `: keepalive` 每 10 秒发送一次（v1.1-P2.4 调整，更快感知死连接）。连接数达到服务器上限（默认 5000，v1.1-P2.4 调整自 1000）时返回 `429 RATE_LIMITED`。

---

## 15. 遥测

**路径前缀：** `/api/telemetry`
**认证：** Bearer Token（必须）
**请求体上限：** 64 KB

---

### POST /api/telemetry

提交客户端遥测数据。

**必须包含的请求头：** `x-device-id`

**请求体：**
```json
{
  "eventType": "periodic",
  "requestId": null,
  "clientTs": "2024-01-15T08:00:00Z",
  "payload": {
    "device": {
      "cpuCores": 8,
      "memoryGb": 16.0,
      "screenWidth": 1920,
      "screenHeight": 1080,
      "pixelRatio": 2.0,
      "osName": "macOS",
      "browserName": "Chrome",
      "browserVersion": "120.0",
      "timezone": "Asia/Shanghai",
      "language": "zh-CN",
      "touchSupport": false,
      "onlineStatus": true
    },
    "sessionDurationSecs": 300,
    "actionsPerMin": 12.5,
    "errorCount": 0,
    "avgResponseTimeMs": 1200.0,
    "behavior": {
      "currentRoute": "/learning",
      "clickCount": 45,
      "clickTargets": [
        { "label": "Submit", "tag": "button" }
      ],
      "scrollDepthPct": 80,
      "visibilityChanges": 2,
      "routeChanges": 3
    },
    "featureUsage": {
      "flashcard": 20,
      "etymology": 3
    }
  }
}
```

**字段说明：**

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `eventType` | string | ✓ | `"session_start"` / `"periodic"` / `"on_demand"` |
| `requestId` | string | 条件必填 | `eventType="on_demand"` 时必须提供 |
| `clientTs` | string | ✓ | 客户端时间（ISO 8601） |
| `payload` | object | ✓ | 见上方结构；`eventType="session_start"` 时通常携带 `payload.device` |

`payload.behavior` 可选字段：

| 字段 | 类型 | 说明 |
|---|---|---|
| `currentRoute` | string | 当前路由 |
| `clickCount` | number | 点击次数 |
| `clickTargets` | array | 点击目标对象列表，元素为 `{ "label": string, "tag": string }` |
| `scrollDepthPct` | number | 页面滚动深度百分比，0–100 |
| `visibilityChanges` | number | 可见性变化次数 |
| `routeChanges` | number | 路由变化次数 |

**响应 200：**
```json
{
  "success": true,
  "data": { "id": "<uuid>" }
}
```

---

## 16. 状态

**路径前缀：** `/api/status`
**认证：** 无

---

### GET /api/status

获取系统状态。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "maintenanceMode": false,
    "version": "v1.1.2-beta.3"
  }
}
```

> `version` 为构建时注入的 git tag 名称（如 `v1.1.2-beta.3`），非严格 semver。

---

### GET /api/status/device-ban

检查设备是否被封禁。

**查询参数：** `deviceId`（必填）

**响应 200：**
```json
{
  "success": true,
  "data": { "banned": false }
}
```

---

## 17. 健康检查

**路径前缀：** `/health`（不在 `/api` 下）
**认证：** 除 `/health/database` 和 `/health/metrics` 外无需认证

---

### GET /health

获取综合健康状态。

**响应 200：**
```json
{
  "status": "ok",
  "uptimeSecs": 86400,
  "services": {
    "store": { "healthy": true },
    "amas": { "healthy": true },
    "sse": {
      "healthy": true,
      "activeConnections": 5,
      "activeDevices": 3
    },
    "wordbookCenter": { "healthy": true, "probeSkipped": false }
  }
}
```

`status`：`"ok"` / `"degraded"` / `"down"`

> **探测范围说明**：`store.healthy` 反映数据库实际探测结果；`amas.healthy` 反映 AMAS 引擎状态（真实检测）；`sse.healthy` 当前版本只做结构探测，返回 `true`；`wordbookCenter.healthy` 在配置了远程 URL 时执行真实 HTTP 探测。`status` 可能为 `"ok"` / `"degraded"` / `"down"`。

---

### GET /health/live

存活探针，始终返回 200。

---

### GET /health/ready

就绪探针，数据库就绪返回 200，否则 503。

---

### GET /health/database

数据库健康详情（需 Admin Token）。

**响应 200：**
```json
{
  "healthy": true,
  "latencyUs": 150,
  "consecutiveFailures": 0
}
```

---

### GET /health/metrics

AMAS 算法指标快照（需 Admin Token）。

---

## 18. V1 兼容层 {#v1-deprecation}

> **⚠️ 已停止维护 — 所有端点返回 410 Gone**
>
> `/api/v1/*` 为历史兼容层，于 **v0.6.0-beta.4（2026-05-21）起停止处理业务逻辑**，全部端点统一返回 **410 Gone**，预计在 **v2.0.0（2027-01-01）随 major bump 一并移除路由**。
>
> - **业务逻辑已冻结**：v1 层不再读写任何数据，不触发 AMAS 自适应引擎，调用方无法再获取单词、记录或会话数据。
> - **运行时弃用信号**：所有 v1 端点响应均携带 `Deprecation` + `Sunset` header（RFC 8594），客户端 logger 应据此告警并引导升级。
> - **现有调用方**：当前 iOS / Web 客户端**均已迁移至 `/api/*` 主端点**，无任何生产流量经过 v1 层。

**路径前缀：** `/api/v1`
**认证：** 无（请求未进入业务逻辑即被 410 拦截）

---

### 统一 410 Gone 响应

以下所有端点均返回 410 Gone，不接受任何请求参数，响应体一致：

- `GET /api/v1/words`
- `GET /api/v1/words/:id`
- `GET /api/v1/records`
- `POST /api/v1/records`
- `GET /api/v1/study-config`
- `POST /api/v1/learning/session`

**响应 410：**
```json
{
  "error": "GONE",
  "message": "/api/v1/* 端点已停止维护，请迁移至 /api/* 主端点",
  "sunset": "Fri, 01 Jan 2027 00:00:00 GMT",
  "migrationDoc": "https://heartcoolman.github.io/wordforge/api-endpoints#v1-deprecation"
}
```

**响应头（RFC 8594，整个 `/api/v1/*` 路由器统一注入）：**

| 响应头 | 值 |
|---|---|
| `Deprecation` | `Wed, 21 May 2026 00:00:00 GMT` |
| `Sunset` | `Fri, 01 Jan 2027 00:00:00 GMT` |
| `Link` | `<https://heartcoolman.github.io/wordforge/api-endpoints#v1-deprecation>; rel="sunset"` |

---

### 迁移对照表

| v1 路径 | 新路径 |
|---|---|
| `GET /api/v1/words` | `GET /api/words` |
| `GET /api/v1/words/:id` | `GET /api/words/:id` |
| `GET /api/v1/records` | `GET /api/records` |
| `POST /api/v1/records` | `POST /api/records` |
| `GET /api/v1/study-config` | `GET /api/study-config` |
| `POST /api/v1/learning/session` | `POST /api/learning/session` |

---

## 19. 单词收藏

**路径前缀：** `/api/word-favorites`
**认证：** Bearer Token（必须）

收藏单词的 CRUD。批量回填星标时优先用 `WordLearningState.bookmarked`（见第 7 节），无需再调 `/status`。

---

### GET /api/word-favorites

获取当前用户的收藏列表（分页）。

**查询参数：** `page`、`perPage`

**响应 200：**
```json
{
  "success": true,
  "data": {
    "data": [
      {
        "wordId": "<uuid>",
        "favorited": true,
        "createdAt": "2024-01-15T08:00:00Z",
        "word": { "id": "<uuid>", "text": "abandon", "meaning": "..." }
      }
    ],
    "total": 1,
    "page": 1,
    "perPage": 20,
    "totalPages": 1
  }
}
```

> `word` 为 `WordPublic` 投影；若收藏的单词已被删除，该条目会被过滤。v0.6.0-beta.4 P3#5 起改为分页响应（`paginated()` 格式），列表字段为 `data.data`（非 `data.items`）。

---

### GET /api/word-favorites/status

批量查询单词的收藏状态。

**查询参数：**

| 参数 | 类型 | 说明 |
|---|---|---|
| `wordIds` | string | 逗号分隔的单词 ID 列表，最多受 `max_batch_size` 限制（默认 500） |

**响应 200：**
```json
{
  "success": true,
  "data": [
    { "wordId": "<id1>", "favorited": true,  "createdAt": "2024-01-15T08:00:00Z" },
    { "wordId": "<id2>", "favorited": false, "createdAt": null }
  ]
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `WORD_FAVORITES_TOO_MANY_IDS` | `wordIds` 数量超过 `max_batch_size` |

---

### POST /api/word-favorites/:wordId

收藏指定单词（幂等：重复收藏返回既有记录）。

**请求体：** 空（单词 ID 在路径中）

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordId": "<uuid>",
    "favorited": true,
    "createdAt": "2024-01-15T08:00:00Z"
  }
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `NOT_FOUND` | 单词不存在 |

---

### DELETE /api/word-favorites/:wordId

取消收藏。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "wordId": "<uuid>",
    "favorited": false,
    "deleted": true
  }
}
```

> `deleted: false` 表示该 word 本就未收藏（幂等）。

---

## 20. 单词笔记

**路径前缀：** `/api/word-notes`
**认证：** Bearer Token（必须）

用户级单词笔记的 CRUD。`content` 长度上限 5000 字符。

**WordNote 结构：**
```json
{
  "id": "<uuid>",
  "wordId": "<uuid>",
  "content": "记忆要点：源自拉丁文 ad+bandon",
  "createdAt": "2024-01-15T08:00:00Z",
  "updatedAt": "2024-01-15T08:00:00Z"
}
```

---

### GET /api/word-notes

获取当前用户的笔记列表，按 `updatedAt DESC` 排序。

**查询参数：**

| 参数 | 类型 | 说明 |
|---|---|---|
| `wordId` | string | 可选；指定后仅返回该单词下的笔记 |

**响应 200：** `WordNote[]`

---

### POST /api/word-notes

创建笔记。

**请求体：**

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `wordId` | string | ✓ | 已存在的单词 ID |
| `content` | string | ✓ | 去除首尾空格后非空，最长 5000 字符 |

**响应 201：** `WordNote` 对象

**错误码：**

| 错误码 | 说明 |
|---|---|
| `NOT_FOUND` | 单词不存在 |
| `WORD_NOTE_EMPTY_CONTENT` | 内容为空或仅含空白字符 |
| `WORD_NOTE_TOO_LONG` | 内容超过 5000 字符 |

---

### PUT /api/word-notes/:id

更新笔记内容。

**请求体：**

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `content` | string | ✓ | 同 POST |

**响应 200：** `WordNote` 对象（`updatedAt` 更新为当前时间）

**错误码：**

| 错误码 | 说明 |
|---|---|
| `NOT_FOUND` | 笔记不存在或不属于当前用户 |
| `WORD_NOTE_EMPTY_CONTENT` / `WORD_NOTE_TOO_LONG` | 同 POST |

---

### DELETE /api/word-notes/:id

删除笔记。

**响应 200：**
```json
{
  "success": true,
  "data": { "deleted": true }
}
```

**错误码：**

| 错误码 | 说明 |
|---|---|
| `NOT_FOUND` | 笔记不存在或不属于当前用户 |

---

## 21. 资源包热更（v1.1）

**路径前缀：** `/api/resource-packs`
**认证：** ❌ 匿名（资源包视作公开内容）
**契约文档：** [`docs/backend-handoff-resource-pack-v1.1.md`](backend-handoff-resource-pack-v1.1.md)

### GET /api/resource-packs

列出所有可见资源包元数据。

**响应 200：**
```json
{
  "success": true,
  "data": [
    {
      "packId": "wordbook-core",
      "description": "核心词库",
      "createdAt": "2026-05-22T10:00:00Z",
      "updatedAt": "2026-05-22T10:00:00Z"
    }
  ]
}
```

---

### GET /api/resource-packs/public-key

返回当前后端 Ed25519 签名公钥（base64，44 字符含 padding）。供客户端 verify SDK 自检；正式分发仍由 v1.1 客户端发版时硬编码。

**响应 200：**
```json
{
  "publicKey": "BASE64_ENCODED_32_BYTES",
  "algorithm": "ed25519"
}
```

**响应头：** `Cache-Control: public, max-age=300`

---

### GET /api/resource-packs/:packId/manifest

返回某 channel 当前激活版本的 manifest。客户端 `ResourcePackManager.check()` 调用入口。

**Query 参数：**

| 名 | 类型 | 必填 | 示例 | 说明 |
|---|---|---|---|---|
| `appVersion` | string (semver) | ✅ | `1.0.0` | 客户端 App marketing version |
| `locale` | string (BCP-47) | ✅ | `zh-Hans-CN` | 客户端 Locale |
| `channel` | string | ⬜ | `stable` | 灰度通道，缺省 `stable`；可选 `stable` / `beta` / `internal` |

**响应 200：**
```json
{
  "packId": "wordbook-core",
  "version": "1.2.3",
  "downloadURL": "https://api.wordforge.app/packs/wordbook-core/1.2.3/payload.json",
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "sizeBytes": 12345,
  "minAppVersion": "1.0.0",
  "channel": "stable",
  "signature": "BASE64_ENCODED_64_BYTES",
  "signatureAlgorithm": "ed25519"
}
```

**响应头：**
- `ETag: "<version>"`
- `Cache-Control: public, max-age=60`

**304 Not Modified：** 客户端带 `If-None-Match: "<version>"` 命中当前 version 时返回空响应（保留 ETag + Cache-Control 头）。

**错误码：**

| code | HTTP | 触发场景 |
|---|---|---|
| `RESOURCE_PACK_NOT_FOUND` | 404 | packId 不存在 / 无激活版本 / 该 channel 未激活 |
| `RESOURCE_PACK_APP_VERSION_TOO_LOW` | 409 | 客户端 appVersion 低于 manifest 的 minAppVersion |
| `VALIDATION_ERROR` | 400 | packId 为空 / channel 非法 |

**downloadURL 推断顺序：**
1. 环境变量 `RESOURCE_PACK_BASE_URL`（部署时硬编码 CDN 前缀）
2. 请求头 `X-Forwarded-Proto` + `X-Forwarded-Host`（反向代理透传）
3. 请求头 `Host`，scheme 默认 `http`

物理路径：`static/packs/<packId>/<version>/payload.json`，通过 `tower-http::ServeDir` 自动托管，命中 `/packs/*` cache 头规则 `public, max-age=31536000, immutable`。

---

### POST /api/telemetry/resource-pack-install

客户端上报资源包安装事件（用于 admin /stats 聚合）。

**认证：** Bearer Token + `X-Device-Id` 请求头

**请求体：**
```json
{
  "packId": "wordbook-core",
  "version": "1.2.3",
  "outcome": "installed",
  "appVersion": "1.0.0"
}
```

| 字段 | 类型 | 必填 | 取值 |
|---|---|---|---|
| `packId` | string | ✅ | 业务 ID |
| `version` | string | ✅ | 已安装的版本 |
| `outcome` | string | ✅ | `installed` / `verify_failed` / `rollback` |
| `appVersion` | string | ⬜ | 客户端 App 版本 |

**响应 200：** `{ "success": true, "data": { "received": true } }`

---

## 22. 资源包管理（admin · v1.1）

**路径前缀：** `/api/admin/resource-packs`
**认证：** AdminAuthUser（独立 admin JWT）

### POST /api/admin/resource-packs/:packId/versions

上传新版资源包。raw body 是 payload bytes，后端自动算 SHA256、Ed25519 签名、落 `static/packs/<pack>/<version>/payload.json`、写表。

**Query 参数：**

| 名 | 类型 | 必填 | 示例 | 说明 |
|---|---|---|---|---|
| `version` | string | ✅ | `1.2.3` | 语义版本，单调递增 |
| `channel` | string | ✅ | `stable` | `stable` / `beta` / `internal` |
| `minAppVersion` | string | ⬜ | `1.0.0` | App 版本下限，客户端低于此值静默忽略 |
| `description` | string | ⬜ | `核心词库` | admin 备注（pack 元数据） |

**请求体：** raw bytes（Content-Type 不限，建议 `application/octet-stream` 或 `application/json`）。**上限 4 MiB。**

**响应 200：**
```json
{
  "success": true,
  "data": {
    "packId": "wordbook-core",
    "version": "1.2.3",
    "sha256": "...",
    "signature": "...",
    "sizeBytes": 12345,
    "channel": "stable"
  }
}
```

**错误码：**

| code | HTTP | 触发场景 |
|---|---|---|
| `PACK_EMPTY` | 400 | body 为空 |
| `PAYLOAD_TOO_LARGE` | 413 | body > 4 MiB |
| `VALIDATION_ERROR` | 400 | packId / version / channel 非法 |
| `RESOURCE_PACK_SIGNER_UNAVAILABLE` | 503 | Ed25519 签名器未初始化（启动失败兜底） |

---

### PUT /api/admin/resource-packs/:packId/channel/:channel/active

切换某 channel 当前激活版本，触发 SSE 广播。

**路径参数：**
- `packId`：业务 ID
- `channel`：`stable` / `beta` / `internal`

**请求体：**
```json
{ "version": "1.2.3" }
```

**响应 200：**
```json
{
  "success": true,
  "data": {
    "packId": "wordbook-core",
    "channel": "stable",
    "version": "1.2.3",
    "activated": true
  }
}
```

**副作用：** 同 packId × channel 5 分钟内只广播 1 次 `resource_pack_available` SSE 事件（dedup）。

---

### GET /api/admin/resource-packs

返回所有 pack 元数据 + 各自全部版本列表。admin UI 用。

---

### GET /api/admin/resource-packs/:packId/stats

返回 install_log 按 `(version, outcome)` 聚合的统计。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "packId": "wordbook-core",
    "stats": [
      { "version": "1.2.3", "outcome": "installed", "count": 1284 },
      { "version": "1.2.3", "outcome": "verify_failed", "count": 3 },
      { "version": "1.2.3", "outcome": "rollback", "count": 1 }
    ]
  }
}
```

---

### DELETE /api/admin/resource-packs/:packId/versions/:version

软删除某版本：从 manifest 路由摘除，物理文件保留供回滚兜底（GC worker 30 天后清盘 —— v1.1 P2 范围）。

**响应 200：**
```json
{
  "success": true,
  "data": {
    "packId": "wordbook-core",
    "version": "1.2.3",
    "deactivated": true
  }
}
```
