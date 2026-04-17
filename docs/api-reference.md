# API 完整参考文档

**基础路径**：`/`（健康检查）和 `/api`（业务接口）  
**协议**：HTTPS（生产环境）  
**数据格式**：JSON（`Content-Type: application/json`）

---

## 认证方式

| 场景 | 方式 |
|---|---|
| 用户接口 | `Authorization: Bearer <accessToken>` 或 `token` Cookie |
| 刷新 token | `refresh_token` Cookie 或 `Authorization: Bearer <refreshToken>` |
| 管理接口 | `Authorization: Bearer <adminToken>` |

---

## 公共格式

### 分页响应

```json
{
  "data": [...],
  "pagination": {
    "page": 1,
    "perPage": 20,
    "total": 500
  }
}
```

### 错误响应

```json
{
  "code": "INVALID_CREDENTIALS",
  "message": "描述信息",
  "details": {}
}
```

### 常用 HTTP 状态码

| 状态码 | 含义 |
|---|---|
| 200 | 成功 |
| 201 | 创建成功 |
| 400 | 请求参数错误 |
| 401 | 未认证 |
| 403 | 权限不足 |
| 404 | 资源不存在 |
| 429 | 请求频率超限 |
| 500 | 服务端错误 |

---

## 1. 健康检查

### `GET /health`

综合健康状态（含依赖服务检查）

```json
{
  "status": "ok | degraded | down",
  "uptimeSecs": 3600,
  "services": {
    "store": "ok | degraded | down",
    "amas": "ok | degraded | down",
    "sse": "ok | degraded | down",
    "wordbookCenter": "ok | degraded | down"
  }
}
```

### `GET /health/live`

存活探针，始终返回 200

### `GET /health/ready`

就绪探针，检查数据库连通性

### `GET /health/database` *(admin)*

数据库延迟检查

```json
{
  "healthy": true,
  "latencyUs": 120,
  "consecutiveFailures": 0
}
```

### `GET /health/metrics` *(admin)*

AMAS 算法指标快照

```json
{ "algorithms": { ... } }
```

---

## 2. 系统状态

### `GET /api/status`

```json
{ "maintenanceMode": false, "version": "1.0.0" }
```

### `GET /api/status/device-ban?deviceId=X`

| Query | 类型 | 必填 |
|---|---|---|
| `deviceId` | string | 是 |

```json
{ "banned": false }
```

---

## 3. 认证（Auth）

> 频率限制：5 次/分钟（按 IP）

### `POST /api/auth/register`

**Body**：`{ email, username, password }`

**Response**：

```json
{
  "accessToken": "...",
  "user": { "id", "email", "username", "isBanned" }
}
```

设置 `token`（httpOnly）和 `refresh_token`（httpOnly）Cookie。

---

### `POST /api/auth/login`

**Body**：`{ email, password }`

**Response**：同 register

---

### `POST /api/auth/refresh`

刷新访问令牌，需携带 `refresh_token` Cookie 或 Bearer refresh token。

**Response**：同 register

---

### `POST /api/auth/logout`

清除 session 和 Cookie。

```json
{ "loggedOut": true }
```

---

### `POST /api/auth/forgot-password`

**Body**：`{ email }`

```json
{ "emailSent": true, "message": "..." }
```

---

### `POST /api/auth/reset-password`

**Body**：`{ token, newPassword }`

---

### `POST /api/auth/verify-reset-token`

**Body**：`{ token }`

```json
{ "valid": true }
```

---

## 4. 管理认证（Admin Auth）

### `POST /api/admin/auth/setup`

初始化第一个管理员（无需已有管理员）

**Body**：`{ email, password }`

```json
{ "token": "...", "admin": { "id", "email" } }
```

---

### `POST /api/admin/auth/login`

**Body**：`{ email, password }` → `{ token, admin }`

### `POST /api/admin/auth/logout`

### `GET /api/admin/auth/verify` *(admin)*

```json
{ "admin": { "id", "email" } }
```

### `GET /api/admin/auth/status` *(public)*

```json
{ "initialized": true }
```

---

## 5. 用户（Users）

### `GET /api/users/me`

```json
{ "id", "email", "username", "isBanned" }
```

### `PUT /api/users/me`

**Body**：`{ username? }`

### `PUT /api/users/me/password`

**Body**：`{ currentPassword, newPassword }` → `{ "passwordChanged": true }`

### `GET /api/users/me/stats`

```json
{
  "totalWordsLearned": 100,
  "totalSessions": 20,
  "totalRecords": 500,
  "streakDays": 7,
  "accuracyRate": 0.85
}
```

---

## 6. 管理用户（Admin Users）

### `GET /api/admin/users`

| Query | 类型 | 说明 |
|---|---|---|
| `page?` | integer | 页码 |
| `perPage?` | integer | 每页数量 |
| `search?` | string | 搜索关键词 |
| `banned?` | boolean | 过滤封禁状态 |

分页用户列表（不含密码字段）

### `POST /api/admin/users/:id/ban`

### `POST /api/admin/users/:id/unban`

### `POST /api/admin/users/:id/reset-password`

```json
{ "resetToken": "..." }
```

### `POST /api/admin/users/:id/set-password`

**Body**：`{ password }` → `{ "passwordSet": true }`

---

## 7. 单词（Words）

### `GET /api/words`

| Query | 类型 | 说明 |
|---|---|---|
| `page?` | integer | 默认 1 |
| `perPage?` | integer | 默认 20 |
| `search?` | string | 搜索词 |

分页返回 `WordPublic` 列表

### `GET /api/words/:id`

返回单个 `WordPublic`

### `GET /api/words/count`

```json
{ "total": 5000 }
```

### `POST /api/words/batch-get`

**Body**：`{ ids: string[] }` → 按请求 ID 顺序返回的 `WordPublic[]`，缺失 ID 自动跳过

---

### `WordPublic` 结构

```json
{
  "id": "uuid",
  "text": "ephemeral",
  "meaning": "短暂的；临时的",
  "pronunciation": "ɪˈfemərəl",
  "partOfSpeech": "adjective",
  "difficulty": 0.6,
  "examples": ["..."],
  "tags": ["GRE"],
  "createdAt": "2026-01-01T00:00:00Z"
}
```

---

### 管理单词操作 *(admin)*

### `POST /api/words`

**Body**：`{ text, meaning, pronunciation?, partOfSpeech?, difficulty?, examples?, tags?, id? }`

返回 201 + `WordPublic`

### `PUT /api/words/:id`

部分更新，同上字段均为可选

### `DELETE /api/words/:id`

```json
{ "deleted": true, "id": "uuid" }
```

### `POST /api/words/batch` *(admin)*

**Body**：`{ words: [UpsertWordRequest] }`

```json
{ "count": 50, "skipped": [2, 5], "items": [...] }
```

### `POST /api/words/import-url` *(admin)*

**Body**：`{ url }` → `{ "imported": 50, "items": [...] }`

> 含 SSRF 防护、DNS 重绑定防护，最大 10MB

---

## 8. 词书（Wordbooks）

### `GET /api/wordbooks/system`

系统词书列表

### `GET /api/wordbooks/user`

当前用户私人词书列表

### `POST /api/wordbooks`

**Body**：`{ name, description? }` → 201 + Wordbook

### `GET /api/wordbooks/:id/words`

| Query | 说明 |
|---|---|
| `page?`, `perPage?` | 分页 |

### `POST /api/wordbooks/:id/words`

**Body**：`{ wordIds: string[] }` → 更新后的 Wordbook

### `DELETE /api/wordbooks/:id/words/:wordId`

---

## 9. 词书中心（Wordbook Center）

### `GET /api/wordbook-center/browse`

浏览远程词书目录（含本地导入状态）

| Query | 说明 |
|---|---|
| `page?`, `perPage?` | 分页 |

### `GET /api/wordbook-center/browse/:id`

预览远程词书（含词条样本）

### `POST /api/wordbook-center/import/:id`

从中心导入词书

### `GET /api/wordbook-center/updates`

可用更新列表

### `POST /api/wordbook-center/updates/:id/sync`

同步词书至最新版本

### `POST /api/wordbook-center/import-url`

**Body**：`{ url }` → 导入后的 Wordbook

### `GET /api/wordbook-center/settings`

### `PUT /api/wordbook-center/settings`

---

## 10. 学习会话（Learning）

### `POST /api/learning/session`

创建或恢复学习会话

**Body**：`{ targetMasteryCount? }`

```json
{
  "sessionId": "uuid",
  "status": "active",
  "resumed": false,
  "targetMasteryCount": 10,
  "crossSessionHint": {}
}
```

---

### `GET /api/learning/study-words`

当前批次学习词

```json
{
  "words": [WordPublic],
  "strategy": {
    "difficultyRange": [0.2, 0.8],
    "newRatio": 0.3,
    "batchSize": 10
  }
}
```

---

### `POST /api/learning/next-words`

**Body**：`{ excludeWordIds, masteredWordIds?, sessionPerformance? }`

```json
{ "words": [WordPublic], "batchSize": 10 }
```

---

### `POST /api/learning/adjust-words`

**Body**：`{ recentPerformance?, userState? }`

```json
{ "adjustedStrategy": { ... } }
```

---

### `POST /api/learning/pick-next-word`

**Body**：`{ activeWordIds, errorWordIds, lastShownMap?, priorityMap? }`

```json
{ "word": WordPublic, "priority": 0.9 }
```

---

### `POST /api/learning/generate-options`

**Body**：`{ wordId, mode, poolWordIds }`

```json
{ "options": ["option1", "option2", "option3", "option4"], "correctIndex": 2 }
```

---

### `POST /api/learning/sync-progress`

**Body**：`{ sessionId, totalQuestions?, contextShifts? }` → 更新后的 Session

---

### `POST /api/learning/complete-session`

**Body**：`{ sessionId, masteredWordIds, errorProneWordIds, avgResponseTimeMs }`

→ 完成后的 Session（含 `summary`）

---

### `LearningSession` 结构

```json
{
  "id": "uuid",
  "userId": "uuid",
  "status": "active | completed",
  "targetMasteryCount": 10,
  "totalQuestions": 50,
  "actualMasteryCount": 8,
  "contextShifts": 2,
  "correctCount": 40,
  "totalCount": 50,
  "createdAt": "...",
  "updatedAt": "...",
  "summary": {}
}
```

---

## 11. 学习配置（Study Config）

### `GET /api/study-config`

### `PUT /api/study-config`

**Body**：`{ selectedWordbookIds?, dailyWordCount?, studyMode?, dailyMasteryTarget? }`

### `GET /api/study-config/today-words`

```json
{ "words": [WordPublic], "target": 20 }
```

### `GET /api/study-config/progress`

```json
{
  "studied": 10,
  "target": 20,
  "new": 5,
  "learning": 3,
  "reviewing": 2,
  "mastered": 8
}
```

---

## 12. 学习记录（Records）

### `GET /api/records`

| Query | 说明 |
|---|---|
| `page?`, `perPage?` | 分页 |

### `POST /api/records`

**Body**：

```json
{
  "clientRecordId": "uuid?",
  "wordId": "uuid",
  "isCorrect": true,
  "responseTimeMs": 1200,
  "sessionId": "uuid?",
  "isQuit": false,
  "dwellTimeMs": 3000,
  "pauseCount": 0,
  "switchCount": 0,
  "retryCount": 0,
  "focusLossDurationMs": 0,
  "interactionDensity": 0.8,
  "pausedTimeMs": 0,
  "hintUsed": false
}
```

**Response**：

```json
{
  "record": LearningRecord,
  "amasResult": { ... },
  "duplicate": false
}
```

---

### `POST /api/records/batch`

**Body**：`{ records: [CreateRecordRequest] }`

```json
{
  "count": 10,
  "failed": 0,
  "partial": false,
  "items": [LearningRecord],
  "errors": []
}
```

### `GET /api/records/statistics`

```json
{ "total": 500, "correct": 420, "accuracy": 0.84 }
```

### `GET /api/records/statistics/enhanced`

```json
{
  "total": 500,
  "correct": 420,
  "accuracy": 0.84,
  "streak": 7,
  "daily": [
    { "date": "2026-04-17", "total": 50, "correct": 42, "accuracy": 0.84 }
  ]
}
```

---

## 13. 词学习状态（Word States）

### `GET /api/word-states/:wordId`

### `POST /api/word-states/batch`

**Body**：`{ wordIds: string[] }` → `WordLearningState[]`

### `GET /api/word-states/due/list`

| Query | 说明 |
|---|---|
| `limit?` | 1–200，默认 50 |

到期待复习词列表

### `GET /api/word-states/stats/overview`

```json
{ "new": 100, "learning": 30, "reviewing": 50, "mastered": 200, "forgotten": 10 }
```

### `POST /api/word-states/batch-update`

**Body**：`{ updates: [{ wordId, state?, masteryLevel? }] }`

原子事务，全部成功或全部回滚。

### `POST /api/word-states/:wordId/mark-mastered`

### `POST /api/word-states/:wordId/reset`

---

### `WordLearningState` 结构

```json
{
  "userId": "uuid",
  "wordId": "uuid",
  "state": "new | learning | reviewing | mastered | forgotten",
  "masteryLevel": 0.75,
  "nextReviewDate": "2026-04-20T09:00:00Z",
  "halfLife": 168,
  "correctStreak": 3,
  "totalAttempts": 10,
  "updatedAt": "..."
}
```

---

## 14. 通知与偏好（Notifications）

### `GET /api/notifications`

| Query | 说明 |
|---|---|
| `limit?` | 返回数量 |
| `unreadOnly?` | boolean |

### `GET /api/notifications/unread-count`

```json
{ "unreadCount": 3 }
```

### `PUT /api/notifications/:id/read`

### `POST /api/notifications/read-all`

```json
{ "markedRead": 3 }
```

### `GET /api/notifications/badges`

```json
[
  {
    "id": "first_word",
    "name": "初学者",
    "description": "学习第一个单词",
    "unlocked": true,
    "progress": 1,
    "unlockedAt": "2026-01-01T00:00:00Z"
  }
]
```

### `GET /api/notifications/preferences`

```json
{
  "theme": "light",
  "language": "zh-CN",
  "notificationEnabled": true,
  "soundEnabled": false
}
```

### `PUT /api/notifications/preferences`

**Body**：`{ theme?, language?, notificationEnabled?, soundEnabled? }`

---

## 15. 用户画像（User Profile）

### `GET /api/user-profile/reward`

```json
{ "rewardType": "standard | explorer | achiever | social" }
```

### `PUT /api/user-profile/reward`

**Body**：`{ rewardType }`

### `GET /api/user-profile/cognitive`

AMAS 认知画像（原始数据）

### `GET /api/user-profile/learning-style`

```json
{ "processingSpeed": 0.7, "memoryCapacity": 0.8, "stability": 0.9 }
```

### `GET /api/user-profile/chronotype`

```json
{ "chronotype": "morning | evening | neutral", "preferredHours": [7, 8, 9] }
```

### `GET /api/user-profile/habit`

```json
{
  "preferredHours": [9, 21],
  "medianSessionLengthMins": 15,
  "sessionsPerDay": 2.3,
  "temporalPerformance": {
    "totalSessions": 120,
    "hourlyStats": [
      {
        "sessionCount": 15,
        "avgAccuracy": 0.85,
        "avgResponseTimeMs": 1200,
        "masteryEfficiency": 0.72
      }
    ]
  }
}
```

`temporalPerformance` 为可选字段，`hourlyStats` 长度为 24（对应 0–23 时）

### `POST /api/user-profile/habit`

**Body**：`{ preferredHours?, medianSessionLengthMins?, sessionsPerDay? }`

### `POST /api/user-profile/avatar`

**Body**：二进制图片（PNG/JPEG/GIF/WebP），最大 512KB

```json
{ "avatarUrl": "https://..." }
```

---

## 16. 内容增强（Content）

### `GET /api/content/etymology/:wordId`

```json
{
  "wordId": "uuid",
  "word": "ephemeral",
  "etymology": "来自希腊语 ephemeros...",
  "roots": ["epi-", "hemera"],
  "generated": true,
  "source": "ai"
}
```

### `GET /api/content/semantic/search`

| Query | 说明 |
|---|---|
| `query` | 必填，搜索词 |
| `limit?` | 返回数量 |

```json
{
  "query": "fleeting",
  "results": [WordPublic],
  "total": 5,
  "method": "semantic | keyword",
  "degraded": false
}
```

### `GET /api/content/word-contexts/:wordId`

```json
{ "wordId", "word", "examples": [...], "contexts": [...] }
```

### `GET /api/content/morphemes/:wordId`

```json
{
  "wordId": "uuid",
  "morphemes": [
    { "text": "epi-", "type": "prefix", "meaning": "upon" }
  ]
}
```

### `POST /api/content/morphemes/:wordId` *(admin)*

**Body**：`{ morphemes: [{ text, type, meaning }] }`

### `GET /api/content/confusion-pairs/:wordId`

| Query | 说明 |
|---|---|
| `limit?` | 1–20，默认 20 |

```json
{
  "wordId": "uuid",
  "confusionPairs": [
    { "wordId", "word", "meaning", "similarity": 0.85 }
  ]
}
```

---

## 17. 实时事件（Realtime SSE）

### `GET /api/realtime/events`

Server-Sent Events 流，15 秒心跳保活

**Headers**：

| Header | 说明 |
|---|---|
| `X-Device-Id` | 可选，设备 ID |
| `X-Device-Platform` | 可选，平台标识 |

**事件类型**：

| 类型 | 说明 |
|---|---|
| `amas_state` | AMAS 算法状态推送 |
| `maintenance` | 维护模式通知 |
| `update_available` | 新版本可用 |
| `telemetry_request` | 服务端请求上报遥测 |
| `banned` | 账户已封禁 |
| `unbanned` | 封禁已解除 |
| `data_corrupted` | 数据异常告警 |

---

## 18. 遥测（Telemetry）

### `POST /api/telemetry`

**Headers**：`X-Device-Id`（必填）

**Body**（最大 64KB）：

```json
{
  "eventType": "on_demand | periodic",
  "requestId": "uuid?",
  "clientTs": "ISO8601",
  "payload": {
    "deviceInfo": {},
    "behaviorTracking": {},
    "featureUsage": {}
  }
}
```

```json
{ "id": "uuid" }
```

---

## 19. 管理分析（Admin Analytics）

> 所有端点需 admin token

### `GET /api/admin/analytics/engagement`

| Query | 说明 |
|---|---|
| `days?` | 1–30，默认 7 |

```json
{
  "activeToday": 42,
  "trend": { "activeToday": 10.5 }
}
```

### `GET /api/admin/analytics/learning`

```json
{
  "totalRecords": 15000,
  "overallAccuracy": 0.82,
  "trend": { "totalRecords": 5.2, "overallAccuracy": -1.1 }
}
```

### `GET /api/admin/analytics/daily-active-users`

| Query | 说明 |
|---|---|
| `days?` | 1–30，默认 7 |

```json
[{ "date": "2026-04-17", "count": 42 }]
```

### `GET /api/admin/analytics/daily-records`

| Query | 说明 |
|---|---|
| `days?` | 1–30，默认 7 |

```json
[{ "date": "2026-04-17", "total": 320, "correct": 251 }]
```

---

## 20. 管理系统（Admin System）

### `GET /api/admin/stats`

```json
{
  "totalUsers": 500,
  "totalRecords": 15000,
  "trend": { "users": 2.0, "records": 5.2 }
}
```

### `GET /api/admin/settings`

```json
{
  "maxUsers": 1000,
  "registrationEnabled": true,
  "maintenanceMode": false,
  "defaultDailyWords": 20,
  "wordbookCenterUrl": "https://..."
}
```

### `PUT /api/admin/settings`

**Body**：`{ maxUsers?, registrationEnabled?, maintenanceMode?, defaultDailyWords?, wordbookCenterUrl? }`

### `POST /api/admin/settings/reload-amas`

```json
{ "reloaded": true }
```

---

## V1 兼容路由

> 为旧版客户端保留，不含 AMAS 计算

| 端点 | 说明 |
|---|---|
| `GET /api/v1/words` | 词列表（V1 格式） |
| `GET /api/v1/words/:id` | 单词详情 |
| `GET /api/v1/records` | 记录列表 |
| `POST /api/v1/records` | 创建记录（无 AMAS） |
| `GET /api/v1/study-config` | 学习配置 |
| `POST /api/v1/learning/session` | 创建会话 |

---

## 频率限制

| 场景 | 限制 |
|---|---|
| 认证端点（`/api/auth/*`） | 5 次/分钟（按 IP） |
| 通用 API | 100–1000 次/分钟（服务端可配置） |
