# 单词接口

所有接口路径前缀为 `/api/words`（单词 CRUD）和 `/api/word-states`（学习状态）。

---

## 单词管理 (`/api/words`)

### GET `/api/words` — 获取单词列表

分页获取单词，支持搜索。

**认证**：普通用户

**Query 参数**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `page` | u64 | 否 | 页码，默认 1 |
| `perPage` | u64 | 否 | 每页数量，默认 20，上限 100 |
| `search` | String | 否 | 搜索关键词 |

**响应** `200`：

```json
{
  "success": true,
  "data": {
    "data": [WordPublic],
    "total": 100,
    "page": 1,
    "perPage": 20,
    "totalPages": 5
  }
}
```

---

### GET `/api/words/count` — 获取单词总数

**认证**：普通用户

**响应** `200`：

```json
{
  "success": true,
  "data": { "total": 100 }
}
```

---

### GET `/api/words/:id` — 获取单个单词

**认证**：普通用户

**路径参数**：`id` — 单词 ID

**响应** `200`：

```json
{
  "success": true,
  "data": WordPublic
}
```

---

### POST `/api/words` — 创建单词

**认证**：管理员

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | String | 否 | 自定义 ID，默认自动生成 UUID |
| `text` | String | 是 | 单词文本（不能为空） |
| `meaning` | String | 是 | 释义（不能为空） |
| `pronunciation` | String | 否 | 发音 |
| `partOfSpeech` | String | 否 | 词性 |
| `difficulty` | f64 | 否 | 难度 0.0-1.0，默认 0.5 |
| `examples` | String[] | 否 | 例句列表 |
| `tags` | String[] | 否 | 标签列表 |

**响应** `201`：

```json
{
  "success": true,
  "data": WordPublic
}
```

---

### PUT `/api/words/:id` — 更新单词

**认证**：管理员

**路径参数**：`id` — 单词 ID

**请求体**：与创建单词相同。`text`/`meaning` 为空字符串时保留原值，`pronunciation`/`partOfSpeech` 为 null 时保留原值，其余字段未提供时保留原值。

**响应** `200`：

```json
{
  "success": true,
  "data": WordPublic
}
```

---

### DELETE `/api/words/:id` — 删除单词

**认证**：管理员

**路径参数**：`id` — 单词 ID

**响应** `200`：

```json
{
  "success": true,
  "data": { "deleted": true, "id": "xxx" }
}
```

---

### POST `/api/words/batch` — 批量创建单词

**认证**：管理员

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `words` | UpsertWordRequest[] | 是 | 单词数组，上限 500（`max_batch_size`） |

跳过 `text` 或 `meaning` 为空的条目。

**响应** `201`：

```json
{
  "success": true,
  "data": {
    "count": 3,
    "skipped": [1],
    "items": [WordPublic]
  }
}
```

---

### POST `/api/words/batch-get` — 批量获取单词

**认证**：普通用户

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `ids` | String[] | 是 | 单词 ID 列表，上限 500 |

**响应** `200`：

```json
{
  "success": true,
  "data": [WordPublic]
}
```

按请求的 `ids` 顺序返回，不存在的 ID 会被过滤。

---

### POST `/api/words/import-url` — 从 URL 导入单词

**认证**：管理员

从远程文本文件导入单词，支持 `word\tmeaning` 或 `word - meaning` 格式，`#` 开头为注释行。

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `url` | String | 是 | 远程文件 URL（仅 http/https，禁止内网地址） |

限制：响应体上限 10MB，导入单词上限 5000 条（`max_import_words`），导入的单词自动添加 `imported` 标签。

**响应** `201`：

```json
{
  "success": true,
  "data": {
    "imported": 10,
    "items": [WordPublic]
  }
}
```

---

## 公共数据结构

### WordPublic

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | String | 单词 ID |
| `text` | String | 单词文本 |
| `meaning` | String | 释义 |
| `pronunciation` | String? | 发音 |
| `partOfSpeech` | String? | 词性 |
| `difficulty` | f64 | 难度 0.0-1.0 |
| `examples` | String[] | 例句 |
| `tags` | String[] | 标签 |
| `createdAt` | DateTime | 创建时间 |

---

## 单词学习状态 (`/api/word-states`)

### GET `/api/word-states/:word_id` — 获取单词学习状态

**认证**：普通用户（返回当前用户的状态）

**路径参数**：`word_id` — 单词 ID

**响应** `200`：

```json
{
  "success": true,
  "data": WordLearningState
}
```

---

### POST `/api/word-states/batch` — 批量查询学习状态

**认证**：普通用户

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `wordIds` | String[] | 是 | 单词 ID 列表，上限 500 |

**响应** `200`：

```json
{
  "success": true,
  "data": [WordLearningState]
}
```

---

### GET `/api/word-states/due/list` — 获取待复习单词列表

**认证**：普通用户

**Query 参数**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `limit` | usize | 否 | 返回数量，默认 50，范围 1-200 |

**响应** `200`：

```json
{
  "success": true,
  "data": [WordLearningState]
}
```

---

### GET `/api/word-states/stats/overview` — 学习状态统计概览

**认证**：普通用户

**响应** `200`：

```json
{
  "success": true,
  "data": {
    "newCount": 0,
    "learning": 0,
    "reviewing": 0,
    "mastered": 0,
    "forgotten": 0
  }
}
```

---

### POST `/api/word-states/:word_id/mark-mastered` — 标记单词为已掌握

**认证**：普通用户

**路径参数**：`word_id` — 单词 ID

将单词状态设为 `MASTERED`，掌握度设为 1.0。若无已有状态则自动创建。

**响应** `200`：

```json
{
  "success": true,
  "data": WordLearningState
}
```

---

### POST `/api/word-states/:word_id/reset` — 重置单词学习状态

**认证**：普通用户

**路径参数**：`word_id` — 单词 ID

将单词状态重置为初始值（`NEW`，掌握度 0，半衰期 24h）。

**响应** `200`：

```json
{
  "success": true,
  "data": WordLearningState
}
```

---

### POST `/api/word-states/batch-update` — 批量更新学习状态

**认证**：普通用户

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `updates` | BatchUpdateItem[] | 是 | 更新列表，上限 500 |

**BatchUpdateItem**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `wordId` | String | 是 | 单词 ID（必须存在） |
| `state` | WordState | 否 | 目标状态 |
| `masteryLevel` | f64 | 否 | 掌握度 0.0-1.0 |

**响应** `200`：

```json
{
  "success": true,
  "data": { "updated": 3 }
}
```

---

## 公共数据结构

### WordLearningState

| 字段 | 类型 | 说明 |
|------|------|------|
| `userId` | String | 用户 ID |
| `wordId` | String | 单词 ID |
| `state` | WordState | 学习状态 |
| `masteryLevel` | f64 | 掌握度 0.0-1.0 |
| `nextReviewDate` | DateTime? | 下次复习时间 |
| `halfLife` | f64 | 记忆半衰期（小时） |
| `correctStreak` | u32 | 连续正确次数 |
| `totalAttempts` | u32 | 总尝试次数 |
| `updatedAt` | DateTime | 更新时间 |

### WordState 枚举

`NEW` | `LEARNING` | `REVIEWING` | `MASTERED` | `FORGOTTEN`
