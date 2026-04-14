# 词书接口

本文档涵盖两组接口：词书管理（`/api/wordbooks`）和词书中心（`/api/wordbook-center` 用户端，`/api/admin/wordbook-center` 管理端）。

---

## 词书管理 (`/api/wordbooks`)

### GET `/api/wordbooks/system` — 获取系统词书列表

返回所有系统词书。

**认证**：普通用户

**响应** `200`：

```json
{
  "success": true,
  "data": [Wordbook]
}
```

---

### GET `/api/wordbooks/user` — 获取用户词书列表

返回当前用户创建的所有词书。

**认证**：普通用户

**响应** `200`：

```json
{
  "success": true,
  "data": [Wordbook]
}
```

---

### POST `/api/wordbooks` — 创建用户词书

**认证**：普通用户

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | String | 是 | 词书名称，不能为空 |
| `description` | String | 否 | 词书描述 |

**响应** `201`：

```json
{
  "success": true,
  "data": Wordbook
}
```

---

### GET `/api/wordbooks/:id/words` — 获取词书中的单词

分页获取指定词书内的单词列表。系统词书所有用户可读，用户词书仅所有者可访问。

**认证**：普通用户

**路径参数**：`id` — 词书 ID

**Query 参数**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `page` | u64 | 否 | 页码，默认 1 |
| `perPage` | u64 | 否 | 每页数量，使用系统分页配置 |

**响应** `200`：

```json
{
  "success": true,
  "data": {
    "data": [WordPublic],
    "total": 50,
    "page": 1,
    "perPage": 20,
    "totalPages": 3
  }
}
```

---

### POST `/api/wordbooks/:id/words` — 批量添加单词到词书

向用户词书中批量添加单词。不可操作系统词书，仅所有者可操作。

**认证**：普通用户

**路径参数**：`id` — 词书 ID

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `wordIds` | String[] | 是 | 单词 ID 数组，数量不超过 `limits.max_batch_size` |

**响应** `200`：

```json
{
  "success": true,
  "data": { "added": 5 }
}
```

---

### DELETE `/api/wordbooks/:id/words/:word_id` — 从词书移除单词

从用户词书中移除指定单词。不可操作系统词书，仅所有者可操作。

**认证**：普通用户

**路径参数**：
- `id` — 词书 ID
- `word_id` — 单词 ID

**响应** `200`：

```json
{
  "success": true,
  "data": { "removed": true }
}
```

---

## 数据结构

### Wordbook

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | String | 词书 ID |
| `name` | String | 名称 |
| `description` | String | 描述 |
| `type` | String | `"System"` 或 `"User"` |
| `userId` | String? | 所属用户 ID，系统词书为 null |
| `wordCount` | u64 | 单词数量 |
| `createdAt` | DateTime | 创建时间 |

### WordPublic

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | String | 单词 ID |
| `text` | String | 单词原文 |
| `meaning` | String | 释义 |
| `pronunciation` | String? | 发音 |
| `partOfSpeech` | String? | 词性 |
| `difficulty` | f64 | 难度系数 |
| `examples` | String[] | 例句 |
| `tags` | String[] | 标签 |
| `createdAt` | DateTime | 创建时间 |

---

## 词书中心 — 用户端 (`/api/wordbook-center`)

词书中心允许用户从远程源浏览、导入和同步词书。用户需先配置个人词书中心 URL。

### GET `/api/wordbook-center/settings` — 获取词书中心设置

**认证**：普通用户

**响应** `200`：

```json
{
  "success": true,
  "data": { "wordbookCenterUrl": "https://example.com/wordbooks" }
}
```

---

### PUT `/api/wordbook-center/settings` — 更新词书中心设置

**认证**：普通用户

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `wordbookCenterUrl` | String? | 否 | 远程词书中心 URL，空字符串或 null 清除设置 |

**响应** `200`：

```json
{
  "success": true,
  "data": { "wordbookCenterUrl": "https://example.com/wordbooks" }
}
```

---

### GET `/api/wordbook-center/browse` — 浏览远程词书目录

从用户配置的远程源获取可用词书列表，包含导入状态。未配置 URL 时返回空数组。

**认证**：普通用户

**响应** `200`：

```json
{
  "success": true,
  "data": [BrowseItem]
}
```

---

### GET `/api/wordbook-center/browse/:id` — 预览远程词书详情

查看指定远程词书的元信息和分页单词列表。

**认证**：普通用户

**路径参数**：`id` — 远程词书 ID

**Query 参数**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `page` | u64 | 否 | 页码，默认 1 |
| `perPage` | u64 | 否 | 每页数量，默认 20，上限 100 |

**响应** `200`：

```json
{
  "success": true,
  "data": {
    "id": "cet4",
    "name": "CET-4 核心词汇",
    "description": "...",
    "wordCount": 2000,
    "coverImage": "https://...",
    "tags": ["CET-4"],
    "version": "1.0.0",
    "author": "...",
    "downloadCount": 500,
    "words": {
      "data": [RemoteWord],
      "total": 2000,
      "page": 1,
      "perPage": 20,
      "totalPages": 100
    }
  }
}
```

---

### POST `/api/wordbook-center/import/:id` — 从远程源导入词书

将远程词书导入为当前用户的用户词书。已导入过的词书会返回 409 冲突。

**认证**：普通用户

**路径参数**：`id` — 远程词书 ID

**响应** `201`：

```json
{
  "success": true,
  "data": {
    "wordbook": Wordbook,
    "wordsImported": 1500,
    "wordsSkipped": 3
  }
}
```

---

### POST `/api/wordbook-center/import-url` — 通过 URL 直接导入词书

通过完整 URL 直接导入一个远程词书 JSON 文件，不依赖词书中心目录。

**认证**：普通用户

**请求体**：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `url` | String | 是 | 词书 JSON 文件的完整 URL |

**响应** `201`：

```json
{
  "success": true,
  "data": {
    "wordbook": Wordbook,
    "wordsImported": 1500,
    "wordsSkipped": 3
  }
}
```

---

### GET `/api/wordbook-center/updates` — 检查可用更新

检查当前用户已导入的词书是否有新版本可用。未配置 URL 或无导入记录时返回空数组。

**认证**：普通用户

**响应** `200`：

```json
{
  "success": true,
  "data": [UpdateInfo]
}
```

---

### POST `/api/wordbook-center/updates/:id/sync` — 同步更新词书

将已导入的词书同步到远程最新版本。会新增、更新、移除单词以匹配远程内容。仅可同步自己导入的词书。

**认证**：普通用户

**路径参数**：`id` — 远程词书 ID

**响应** `200`：

```json
{
  "success": true,
  "data": {
    "wordbook": Wordbook,
    "wordsAdded": 10,
    "wordsUpdated": 5,
    "wordsRemoved": 2
  }
}
```

---

## 词书中心 — 管理端 (`/api/admin/wordbook-center`)

管理端接口使用系统级词书中心 URL（通过管理后台设置配置），导入的词书类型为 System。

### GET `/api/admin/wordbook-center/browse` — 浏览远程词书目录

**认证**：管理员

**响应** `200`：与用户端 browse 响应结构相同。

---

### GET `/api/admin/wordbook-center/browse/:id` — 预览远程词书详情

**认证**：管理员

**路径参数**：`id` — 远程词书 ID

**Query 参数**：同用户端 browse/:id。

**响应** `200`：与用户端 browse/:id 响应结构相同。

---

### POST `/api/admin/wordbook-center/import/:id` — 导入为系统词书

将远程词书导入为系统词书（`WordbookType::System`，无 `userId`）。

**认证**：管理员

**路径参数**：`id` — 远程词书 ID

**响应** `201`：与用户端 import 响应结构相同。

---

### GET `/api/admin/wordbook-center/updates` — 检查系统词书更新

检查管理员导入的系统词书是否有新版本。

**认证**：管理员

**响应** `200`：与用户端 updates 响应结构相同。

---

### POST `/api/admin/wordbook-center/updates/:id/sync` — 同步系统词书

**认证**：管理员

**路径参数**：`id` — 远程词书 ID

**响应** `200`：与用户端 sync 响应结构相同。

---

## 词书中心数据结构

### BrowseItem

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | String | 远程词书 ID |
| `name` | String | 名称 |
| `description` | String | 描述 |
| `wordCount` | u64 | 单词数量 |
| `coverImage` | String? | 封面图片 URL |
| `tags` | String[] | 标签 |
| `version` | String | 版本号 |
| `author` | String? | 作者 |
| `downloadCount` | u64? | 下载次数 |
| `imported` | bool | 是否已导入 |
| `localWordbookId` | String? | 本地词书 ID（已导入时） |
| `localVersion` | String? | 本地已导入的版本 |
| `hasUpdate` | bool | 是否有新版本可用 |

### RemoteWord

| 字段 | 类型 | 说明 |
|------|------|------|
| `spelling` | String | 单词拼写 |
| `phonetic` | String? | 音标 |
| `meanings` | String[] | 释义列表 |
| `examples` | String[] | 例句列表 |
| `audioUrl` | String? | 音频 URL |

### UpdateInfo

| 字段 | 类型 | 说明 |
|------|------|------|
| `remoteId` | String | 远程词书 ID |
| `name` | String | 词书名称 |
| `localVersion` | String | 本地版本 |
| `remoteVersion` | String | 远程最新版本 |
| `localWordbookId` | String | 本地词书 ID |
