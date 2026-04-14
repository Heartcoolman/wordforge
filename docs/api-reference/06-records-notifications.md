# 学习记录、通知与内容接口

## 学习记录 (`/api/records`)

### GET `/api/records` — 分页查询学习记录

需要认证。返回当前用户的学习记录列表，按分页方式返回。

**Query 参数**

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `page` | `u64` | 否 | `1` | 页码，最小值 1 |
| `perPage` | `u64` | 否 | `50` | 每页数量，范围 1-100 |

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "data": [
      {
        "id": "string",
        "userId": "string",
        "wordId": "string",
        "isCorrect": true,
        "responseTimeMs": 1200,
        "sessionId": "string | null",
        "createdAt": "2024-01-01T00:00:00Z"
      }
    ],
    "total": 100,
    "page": 1,
    "perPage": 50,
    "totalPages": 2
  }
}
```

---

### POST `/api/records` — 提交单条学习记录

需要认证。提交一条学习答题记录，触发 AMAS 引擎处理并更新单词掌握状态、ELO 评分和学习会话统计。支持幂等：若 `clientRecordId` 已存在则返回已有记录且 `duplicate: true`。

**请求体**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `clientRecordId` | `String` | 否 | 客户端生成的记录 ID，用于幂等去重；缺省时服务端生成 UUID |
| `wordId` | `String` | 是 | 单词 ID |
| `isCorrect` | `bool` | 是 | 本次作答是否正确 |
| `responseTimeMs` | `i64` | 是 | 作答用时（毫秒） |
| `sessionId` | `String` | 否 | 关联的学习会话 ID |
| `isQuit` | `bool` | 否 | 是否中途退出，默认 `false` |
| `dwellTimeMs` | `i64` | 否 | 停留时长（毫秒） |
| `pauseCount` | `i32` | 否 | 暂停次数 |
| `switchCount` | `i32` | 否 | 切换次数 |
| `retryCount` | `i32` | 否 | 重试次数 |
| `focusLossDurationMs` | `i64` | 否 | 失焦时长（毫秒） |
| `interactionDensity` | `f64` | 否 | 交互密度 |
| `pausedTimeMs` | `i64` | 否 | 暂停总时长（毫秒） |
| `hintUsed` | `bool` | 否 | 是否使用提示，默认 `false` |

**响应体** — `201 Created`（新建）/ `200 OK`（重复）

```json
{
  "success": true,
  "data": {
    "record": {
      "id": "string",
      "userId": "string",
      "wordId": "string",
      "isCorrect": true,
      "responseTimeMs": 1200,
      "sessionId": "string | null",
      "createdAt": "2024-01-01T00:00:00Z"
    },
    "amasResult": {
      "sessionId": "string",
      "strategy": { },
      "explanation": { },
      "state": { },
      "wordMastery": { } ,
      "reward": { },
      "coldStartPhase": "Classify | Explore | null"
    },
    "duplicate": false
  }
}
```

当 `duplicate` 为 `true` 时，`amasResult` 为 `null`。

---

### POST `/api/records/batch` — 批量提交学习记录

需要认证。批量提交学习记录，每条记录独立处理。数量上限由服务端配置 `limits.max_batch_size` 决定。全部失败时自动回滚用户级引擎状态。

**请求体**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `records` | `CreateRecordRequest[]` | 是 | 学习记录数组，每条结构同上 |

**响应体** — `201 Created`（有新记录）/ `200 OK`（部分失败或全为重复）

```json
{
  "success": true,
  "data": {
    "count": 5,
    "failed": 1,
    "partial": true,
    "items": [ ],
    "errors": [
      { "index": 2, "code": "string", "message": "string" }
    ]
  }
}
```

| 字段 | 说明 |
|------|------|
| `count` | 成功处理的记录数 |
| `failed` | 失败的记录数 |
| `partial` | 是否存在部分失败 |
| `items` | 成功的 `CreateRecordResponse` 数组 |
| `errors` | 失败记录的错误详情，包含原始数组索引 |

---

### GET `/api/records/statistics` — 基础统计

需要认证。返回当前用户的学习记录基础统计数据。

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "total": 500,
    "correct": 420,
    "accuracy": 0.84
  }
}
```

---

### GET `/api/records/statistics/enhanced` — 增强统计

需要认证。返回增强统计数据，包含按日拆分和连续学习天数。查询量受服务端 `limits.max_stats_records` 限制。

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "total": 500,
    "correct": 420,
    "accuracy": 0.84,
    "streak": 7,
    "daily": [
      {
        "date": "2024-01-15",
        "total": 30,
        "correct": 25,
        "accuracy": 0.833
      }
    ]
  }
}
```

| 字段 | 说明 |
|------|------|
| `streak` | 连续学习天数（从今天或昨天向前计算） |
| `daily` | 按日拆分的统计数组，按日期升序排列 |

---

## 通知 (`/api/notifications`)

### GET `/api/notifications` — 查询通知列表

需要认证。返回当前用户的通知列表。

**Query 参数**

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `limit` | `usize` | 否 | `50` | 返回数量，范围 1-200 |
| `unreadOnly` | `bool` | 否 | `false` | 是否只返回未读通知 |

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": [
    {
      "id": "string",
      "userId": "string",
      "type": "System | Achievement | Reminder | Info | Broadcast",
      "title": "string",
      "message": "string",
      "wordId": "string | null",
      "overdueHours": 48,
      "read": false,
      "createdAt": "2024-01-01T00:00:00Z"
    }
  ]
}
```

---

### GET `/api/notifications/unread-count` — 未读通知数量

需要认证。

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": { "unreadCount": 3 }
}
```

---

### PUT `/api/notifications/:id/read` — 标记单条已读

需要认证。将指定通知标记为已读。通知不存在时返回 404。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | `String` | 通知 ID |

**响应体** — `200 OK`

返回被标记的 `Notification` 对象。

---

### POST `/api/notifications/read-all` — 全部标记已读

需要认证。将当前用户所有未读通知标记为已读。

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": { "markedRead": 5 }
}
```

---

### GET `/api/notifications/badges` — 徽章列表

需要认证。返回当前用户的徽章进度和解锁状态。包含三种内置徽章：`first_word`（首次学习）、`streak_7`（连续7天）、`mastered_100`（掌握100词）。

**响应体** — `200 OK`

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
      "unlockedAt": "2024-01-01T00:00:00Z"
    }
  ]
}
```

---

### GET `/api/notifications/preferences` — 获取用户偏好设置

需要认证。返回当前用户的主题、语言和通知偏好。

**响应体** — `200 OK`

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

### PUT `/api/notifications/preferences` — 更新用户偏好设置

需要认证。部分更新用户偏好，未提供的字段保持不变。

**请求体**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `theme` | `String` | 否 | 可选值：`light`、`dark`、`system` |
| `language` | `String` | 否 | 可选值：`en`、`zh`、`ja`、`ko`、`fr`、`de`、`es` |
| `notificationEnabled` | `bool` | 否 | 是否启用通知 |
| `soundEnabled` | `bool` | 否 | 是否启用音效 |

**响应体** — `200 OK`

返回更新后的完整 `UserPreferences` 对象（结构同 GET）。

**错误码**

| 错误码 | 说明 |
|--------|------|
| `INVALID_THEME` | 主题值不合法 |
| `INVALID_LANGUAGE` | 语言值不合法 |

---

## 内容 (`/api/content`)

### GET `/api/content/etymology/:word_id` — 获取词源信息

需要认证。返回指定单词的词源分析。优先使用缓存数据，缓存未命中时基于词素规则生成 fallback 说明并缓存。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `word_id` | `String` | 单词 ID |

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "wordId": "string",
    "word": "string",
    "etymology": "词源说明文本",
    "roots": ["un", "break", "able"],
    "generated": false,
    "source": "rule_based_fallback"
  }
}
```

| 字段 | 说明 |
|------|------|
| `generated` | 是否由 LLM 生成（当前 fallback 模式固定为 `false`） |
| `source` | 数据来源标识 |

---

### GET `/api/content/semantic/search` — 语义搜索

需要认证。对单词进行语义搜索。当前降级为关键词匹配模式。

**Query 参数**

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `query` | `String` | 是 | — | 搜索关键词，不能为空 |
| `limit` | `usize` | 否 | `10` | 返回数量，范围 1-50 |

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "query": "string",
    "results": [
      {
        "id": "string",
        "text": "string",
        "meaning": "string",
        "pronunciation": "string | null",
        "partOfSpeech": "string | null",
        "difficulty": 0.5,
        "examples": [],
        "tags": [],
        "createdAt": "2024-01-01T00:00:00Z"
      }
    ],
    "total": 10,
    "method": "keyword_fallback",
    "degraded": true
  }
}
```

| 字段 | 说明 |
|------|------|
| `method` | 搜索方式，当前固定 `keyword_fallback` |
| `degraded` | 是否为降级模式，当前固定 `true` |

---

### GET `/api/content/word-contexts/:word_id` — 获取单词上下文

需要认证。返回指定单词的例句和上下文信息，基于词条的 `examples` 字段生成。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `word_id` | `String` | 单词 ID |

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "wordId": "string",
    "word": "string",
    "examples": ["example sentence 1"],
    "contexts": [
      {
        "id": "wordId-ctx-0",
        "sentence": "example sentence 1",
        "source": "word_examples"
      }
    ]
  }
}
```

---

### GET `/api/content/morphemes/:word_id` — 获取单词词素

需要认证。返回指定单词的词素拆分信息。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `word_id` | `String` | 单词 ID |

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "wordId": "string",
    "morphemes": [
      {
        "text": "un",
        "type": "prefix",
        "meaning": "not"
      }
    ]
  }
}
```

---

### POST `/api/content/morphemes/:word_id` — 设置单词词素

需要**管理员认证**。设置指定单词的词素拆分数据。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `word_id` | `String` | 单词 ID |

**请求体**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `morphemes` | `Morpheme[]` | 是 | 词素数组 |

`Morpheme` 结构：

| 字段 | 类型 | 说明 |
|------|------|------|
| `text` | `String` | 词素文本 |
| `type` | `String` | 类型：`prefix`、`root`、`suffix` |
| `meaning` | `String` | 词素含义 |

**响应体** — `200 OK`

返回 `WordMorphemes` 对象（结构同 GET）。

---

### GET `/api/content/confusion-pairs/:word_id` — 获取易混淆词对

需要认证。返回与指定单词容易混淆的其他单词列表。

**路径参数**

| 参数 | 类型 | 说明 |
|------|------|------|
| `word_id` | `String` | 单词 ID |

**Query 参数**

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `limit` | `usize` | 否 | `20` | 返回数量，范围 1-100 |

**响应体** — `200 OK`

```json
{
  "success": true,
  "data": {
    "wordId": "string",
    "confusionPairs": [
      {
        "wordId": "string",
        "word": "string",
        "meaning": "string",
        "similarity": 0.85
      }
    ]
  }
}
```
