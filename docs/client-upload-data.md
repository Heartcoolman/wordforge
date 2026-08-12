# 客户端上传数据规范

> 本文档专门整理所有需要客户端向服务端上传数据的接口，包含完整字段定义、类型约束、校验规则，以及特殊上传流程说明。
> 通用规范（认证、错误码等）见《API 对接规范文档》。

---

## 目录

0. [字段类型说明](#0-字段类型说明)
1. [认证模块](#1-认证模块)
2. [用户模块](#2-用户模块)
3. [学习记录模块](#3-学习记录模块)
4. [学习会话模块](#4-学习会话模块)
5. [单词学习状态模块](#5-单词学习状态模块)
6. [单词本模块](#6-单词本模块)
7. [单词本中心模块](#7-单词本中心模块)
8. [学习配置模块](#8-学习配置模块)
9. [用户档案模块](#9-用户档案模块)
10. [通知偏好模块](#10-通知偏好模块)
11. [AMAS 自适应引擎模块](#11-amas-自适应引擎模块)
12. [遥测模块](#12-遥测模块)
13. [单词管理模块（Admin）](#13-单词管理模块-admin)
14. [内容管理模块（Admin）](#14-内容管理模块-admin)
15. [单词收藏模块](#15-单词收藏模块)
16. [单词笔记模块](#16-单词笔记模块)

---

## 0. 字段类型说明

| 类型标记 | 含义 |
|---|---|
| `string` | UTF-8 字符串 |
| `number` | 数字（整数或浮点） |
| `boolean` | `true` / `false` |
| `string[]` | 字符串数组 |
| `object` | JSON 对象 |
| `null` | 可为空值 |
| `enum(A\|B)` | 枚举，只能是列出的值之一 |
| `✓` | 必填字段 |
| `—` | 可选字段 |

---

## 1. 认证模块

### 1.1 注册

**接口：** `POST /api/auth/register`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `email` | string | ✓ | RFC 5321 格式；最大 254 字符；本地部分最大 64 字符；允许 `a-z A-Z 0-9 . _ + -`；域名需含点 |
| `username` | string | ✓ | 2–50 字符；允许 Unicode 字母数字（含中日韩等）、`_`、`-`、空格 |
| `password` | string | ✓ | 8–256 字符；必须同时包含大写字母、小写字母、数字 |

**示例：**
```json
{
  "email": "alice@example.com",
  "username": "Alice_01",
  "password": "MyPass1!"
}
```

---

### 1.2 登录

**接口：** `POST /api/auth/login`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `email` | string | ✓ | 注册邮箱 |
| `password` | string | ✓ | 账号密码 |

---

### 1.3 发起密码重置

**接口：** `POST /api/auth/forgot-password`

> 当前版本邮件发送功能未启用；服务端会创建重置 token 并写入诊断日志，响应中不会返回 token。

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `email` | string | ✓ | 注册邮箱 |

---

### 1.4 验证重置 Token

**接口：** `POST /api/auth/verify-reset-token`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `token` | string | ✓ | 重置令牌；当前版本不会通过响应或邮件下发 |

---

### 1.5 执行密码重置

**接口：** `POST /api/auth/reset-password`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `token` | string | ✓ | 重置令牌（一次性，有效期 1 小时） |
| `newPassword` | string | ✓ | 8–256 字符；必须同时包含大写字母、小写字母、数字 |

---

## 2. 用户模块

### 2.1 更新用户名

**接口：** `PUT /api/users/me`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `username` | string | — | 2–50 字符；允许 Unicode 字母数字、`_`、`-`、空格 |

---

### 2.2 修改密码

**接口：** `PUT /api/users/me/password`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `currentPassword` | string | ✓ | 当前有效密码 |
| `newPassword` | string | ✓ | 8–256 字符；必须同时包含大写字母、小写字母、数字 |

---

## 3. 学习记录模块

### 3.1 创建单条记录

**接口：** `POST /api/records`
**说明：** 每条记录触发 AMAS 自适应引擎更新，并同步更新单词学习状态和 ELO 评级。

| 字段 | 类型 | 必填 | 约束/说明 |
|---|---|---|---|
| `wordId` | string | ✓ | 已存在的单词 ID |
| `isCorrect` | boolean | ✓ | 本次答题是否正确 |
| `responseTimeMs` | number | ✓ | 从题目呈现到答题的时间，单位毫秒；建议 > 0 |
| `clientRecordId` | string | — | 客户端自生成的幂等 ID（推荐 UUID）；持久存储，同一 `(userId, clientRecordId)` 再次提交将返回已有记录（无时间窗口） |
| `sessionId` | string | — | 所属学习会话 ID；来自 `POST /api/learning/session` 的响应 |
| `isQuit` | boolean | — | 用户是否在此题中途退出；默认 `false` |
| `dwellTimeMs` | number | — | 用户在此题停留的总时间（含阅读、思考），单位毫秒 |
| `pauseCount` | number | — | 答题过程中的暂停次数 |
| `switchCount` | number | — | 答题过程中应用切换（上下文切换）次数 |
| `retryCount` | number | — | 本题重试次数 |
| `focusLossDurationMs` | number | — | 焦点丢失（如切换 App）的累计时长，单位毫秒 |
| `interactionDensity` | number | — | 交互密度（单位时间内操作次数），浮点数 |
| `pausedTimeMs` | number | — | 暂停状态的总累计时长，单位毫秒 |
| `hintUsed` | boolean | — | 是否使用了提示功能；默认 `false` |
| `confusedWith` | string | — | 易混淆的单词 ID，用于 IAD 混淆词对追踪；被标记后两词间会产生干扰衰减，延长复习间隔 |
| `recordType` | enum | — | `"learning"` / `"review"` / `"all"`；按客户端所在 tab 标注（学习 tab → `learning`，复习 tab → `review`），不传则后端落库 `"all"`，老客户端零变更 |

**完整示例（含行为指标）：**
```json
{
  "wordId": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "isCorrect": true,
  "responseTimeMs": 1850,
  "clientRecordId": "client-abc123",
  "sessionId": "sess-uuid-here",
  "isQuit": false,
  "dwellTimeMs": 3200,
  "pauseCount": 0,
  "switchCount": 1,
  "retryCount": 0,
  "focusLossDurationMs": 500,
  "interactionDensity": 1.2,
  "pausedTimeMs": 0,
  "hintUsed": false,
  "recordType": "learning"
}
```

**最简示例（仅必填字段）：**
```json
{
  "wordId": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "isCorrect": true,
  "responseTimeMs": 1850
}
```

**响应要点（v1.3.0 起默认异步）：** `RECORDS_OUTBOX_ASYNC` 默认 `true` 时返回 **202 Accepted** 裸响应 `{accepted, async, clientRecordId}`（无 `record` / `amasResult`）。设 `RECORDS_OUTBOX_ASYNC=false` 回退同步路径：新记录返回 `amasResult`（含 `sessionId`、`strategy`、`explanation`、`state`、`wordMastery`、`reward`、`coldStartPhase`）；重复提交时 `amasResult` 为 `null`。客户端须容忍两种形状（iOS / Android / Web ≥ 1.6.0 已适配）。

---

### 3.2 批量创建记录

**接口：** `POST /api/records/batch`
**说明：** 用于离线缓存同步，最大条数受 `max_batch_size` 限制（默认 500）。

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `records` | array | ✓ | 数组元素结构同 3.1；最大 500 条 |

```json
{
  "records": [
    { "wordId": "<id1>", "isCorrect": true,  "responseTimeMs": 1200, "recordType": "learning" },
    { "wordId": "<id2>", "isCorrect": false, "responseTimeMs": 3500, "hintUsed": true, "recordType": "review" }
  ]
}
```

> 客户端在"学习" tab 上传 `recordType: "learning"`，"复习" tab 上传 `recordType: "review"`；不传走兜底 `"all"`。该字段不影响 AMAS 处理，仅供后续统计接口 `?category=` 过滤使用。

---

## 4. 学习会话模块

### 4.1 创建/恢复会话

**接口：** `POST /api/learning/session`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `targetMasteryCount` | number | — | 本次会话目标掌握词数；若不传则取学习配置中的 `dailyMasteryTarget` |

---

### 4.2 获取下一批单词

**接口：** `POST /api/learning/next-words`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `excludeWordIds` | string[] | ✓ | 本轮已展示过、需排除的单词 ID；最大 1000 条 |
| `masteredWordIds` | string[] | — | 已确认掌握的单词 ID（触发状态更新至 MASTERED） |
| `sessionPerformance` | object | — | 本次会话实时表现（见下表） |

`sessionPerformance` 字段：

| 子字段 | 类型 | 说明 |
|---|---|---|
| `recentAccuracy` | number (0–1) | 近期答题准确率 |
| `masteredCount` | number | 本次会话已掌握数量 |
| `targetMasteryCount` | number | 本次会话目标掌握数量 |
| `errorProneWordIds` | string[] | 本次会话易错单词 ID 列表 |

```json
{
  "excludeWordIds": ["<id1>", "<id2>"],
  "masteredWordIds": ["<id3>"],
  "sessionPerformance": {
    "recentAccuracy": 0.75,
    "masteredCount": 5,
    "targetMasteryCount": 10,
    "errorProneWordIds": ["<id4>"]
  }
}
```

---

### 4.3 调整学习策略

**接口：** `POST /api/learning/adjust-words`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `recentPerformance` | number | — | 最近答题准确率，范围 [0.0, 1.0]，必须为有限数 |
| `userState` | string | — | 用户状态枚举（见下表） |

`userState` 枚举值及效果：

| 值 | AMAS 效果 |
|---|---|
| `focused` | 提高难度 + 提高新词比例 |
| `engaged` | 同 `focused` |
| `confident` | 同 `focused` |
| `tired` | 降低难度 + 减小批次 |
| `fatigued` | 同 `tired` |
| `frustrated` | 同 `tired` |
| `distracted` | 同 `tired` |
| `review` | 进入复习模式，新词比例设为 0 |
| `sprint` | 冲刺模式，新词比例最大化 |

```json
{
  "recentPerformance": 0.65,
  "userState": "tired"
}
```

---

### 4.4 同步会话进度

**接口：** `POST /api/learning/sync-progress`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `sessionId` | string | ✓ | 当前会话 ID |
| `totalQuestions` | number | — | 本次会话累计答题数（服务端只增不减） |
| `contextShifts` | number | — | 上下文切换次数 |

---

### 4.5 完成会话

**接口：** `POST /api/learning/complete-session`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `sessionId` | string | ✓ | 当前会话 ID |
| `masteredWordIds` | string[] | ✓ | 本次会话中确认掌握的所有单词 ID |
| `errorProneWordIds` | string[] | ✓ | 本次会话中频繁出错的单词 ID |
| `avgResponseTimeMs` | number | ✓ | 本次会话平均答题时间，单位毫秒 |

```json
{
  "sessionId": "sess-uuid-here",
  "masteredWordIds": ["<id1>", "<id2>", "<id3>"],
  "errorProneWordIds": ["<id4>"],
  "avgResponseTimeMs": 1650
}
```

---

### 4.6 选取下一个单词

**接口：** `POST /api/learning/pick-next-word`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `activeWordIds` | string[] | ✓ | 当前活跃单词集，非空；最大 500 条 |
| `errorWordIds` | string[] | ✓ | 易错单词集（优先展示） |
| `lastShownMap` | object | — | `{wordId: timestampMs}` 各词上次展示的 Unix 毫秒时间戳 |
| `priorityMap` | object | — | `{wordId: priority}` 各词优先级（整数，越大越优先） |

```json
{
  "activeWordIds": ["<id1>", "<id2>", "<id3>"],
  "errorWordIds": ["<id2>"],
  "lastShownMap": {
    "<id1>": 1705296000000,
    "<id2>": 1705296060000
  },
  "priorityMap": {
    "<id1>": 1,
    "<id2>": 3
  }
}
```

---

### 4.7 生成多选题选项

**接口：** `POST /api/learning/generate-options`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `wordId` | string | ✓ | 目标单词 ID（正确答案来源） |
| `mode` | string | ✓ | `"word-to-meaning"`（看单词猜意思）或 `"meaning-to-word"`（看意思猜单词） |
| `poolWordIds` | string[] | ✓ | 干扰项候选池单词 ID 列表；最大 500 条 |

```json
{
  "wordId": "<targetId>",
  "mode": "word-to-meaning",
  "poolWordIds": ["<id2>", "<id3>", "<id4>", "<id5>"]
}
```

---

## 5. 单词学习状态模块

### 5.1 批量查询状态

**接口：** `POST /api/word-states/batch`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `wordIds` | string[] | ✓ | 最大 500 条 |

---

### 5.2 标记单词为已掌握

**接口：** `POST /api/word-states/:wordId/mark-mastered`
**说明：** 将指定单词标记为已掌握（masteryLevel=1.0）。若记录不存在则自动创建。

**请求体：** 空（单词 ID 在路径中）

---

### 5.3 重置单词学习进度

**接口：** `POST /api/word-states/:wordId/reset`
**说明：** 重置指定单词的学习进度（回到 NEW 状态）。

**请求体：** 空（单词 ID 在路径中）

---

### 5.4 批量更新状态

**接口：** `POST /api/word-states/batch-update`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `updates` | array | ✓ | 更新条目数组，见下表；最大 500 条 |

`updates` 数组元素：

| 子字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `wordId` | string | ✓ | 必须存在于单词库；若该用户尚无学习状态，服务端会自动以默认值初始化 |
| `state` | enum | — | `new` / `learning` / `reviewing` / `mastered` / `forgotten` |
| `masteryLevel` | number | — | 范围 [0.0, 1.0] |

> `WordLearningState.state` 为 lowercase，与 `WordMasteryDecision.masteryLevel` 的 SCREAMING_SNAKE 是不同枚举（与 api-endpoints.md §7、api-spec.md §10 一致）。

```json
{
  "updates": [
    { "wordId": "<id1>", "state": "mastered", "masteryLevel": 1.0 },
    { "wordId": "<id2>", "masteryLevel": 0.6 }
  ]
}
```

---

## 6. 单词本模块

### 6.1 创建自定义单词本

**接口：** `POST /api/wordbooks`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `name` | string | ✓ | 去除首尾空格后非空 |
| `description` | string | — | 单词本描述 |

---

### 6.2 向单词本添加单词

**接口：** `POST /api/wordbooks/:id/words`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `wordIds` | string[] | ✓ | 最大 500 条；元素为单词 ID（服务端不校验单词是否存在） |

```json
{
  "wordIds": ["<id1>", "<id2>", "<id3>"]
}
```

---

## 7. 单词本中心模块

### 7.1 设置单词本中心 URL

**接口：** `PUT /api/wordbook-center/settings`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `wordbookCenterUrl` | string \| null | ✓ | 公网 HTTP/HTTPS URL；`null` 清空配置 |

---

### 7.2 从 URL 导入词库

**接口：** `POST /api/wordbook-center/import-url`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `url` | string | ✓ | 公网 HTTP/HTTPS URL；禁止内网/localhost |

---

## 8. 学习配置模块

### 8.1 更新学习配置

**接口：** `PUT /api/study-config`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `selectedWordbookIds` | string[] | — | 选用的单词本 ID 列表，不限系统或自定义（ID 必须存在） |
| `dailyWordCount` | number | — | 每日学习单词数，范围 [1, 200] |
| `studyMode` | enum | — | `normal` / `intensive` / `review` / `casual` |
| `dailyMasteryTarget` | number | — | 每日掌握目标词数，范围 [1, 100] |

---

## 9. 用户档案模块

### 9.1 设置奖励偏好

**接口：** `PUT /api/user-profile/reward`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `rewardType` | enum | ✓ | `standard` / `explorer` / `achiever` / `social` |

---

### 9.2 更新学习习惯

**接口：** `POST /api/user-profile/habit`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `preferredHours` | number[] | — | 每个元素范围 [0, 23]，代表一天中的整点小时数 |
| `medianSessionLengthMins` | number | — | 单次学习时长中位数，范围 [1, 480] 分钟 |
| `sessionsPerDay` | number | — | 每日学习次数，范围 [1, 20] |

错误码：`INVALID_PREFERRED_HOURS`、`INVALID_SESSION_LENGTH`、`INVALID_SESSIONS_PER_DAY`。

```json
{
  "preferredHours": [8, 12, 20],
  "medianSessionLengthMins": 20,
  "sessionsPerDay": 3
}
```

---

### 9.3 上传头像

**接口：** `POST /api/user-profile/avatar`
**Content-Type：** 直接上传二进制图片（不使用 `multipart/form-data`）

| 项目 | 约束 |
|---|---|
| 最大文件大小 | 512 KB |
| 支持格式 | PNG / JPEG / GIF / WebP |
| 传输方式 | 请求体为原始二进制图片字节 |

**示例（伪代码）：**
```
POST /api/user-profile/avatar
Authorization: Bearer <token>
Content-Type: image/png
Content-Length: 102400

<二进制图片数据>
```

---

## 10. 通知偏好模块

### 10.1 标记通知为已读

**接口：** `PUT /api/notifications/:id/read`
**说明：** 将指定通知标为已读。

**请求体：** 空（通知 ID 在路径中）

---

### 10.2 全部标为已读

**接口：** `POST /api/notifications/read-all`
**说明：** 将所有通知标为已读。

**请求体：** 空

---

### 10.3 更新 UI 偏好

**接口：** `PUT /api/notifications/preferences`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `theme` | enum | — | `light` / `dark` / `system` |
| `language` | enum | — | `en` / `zh` / `ja` / `ko` / `fr` / `de` / `es` |
| `notificationEnabled` | boolean | — | 是否开启通知 |
| `soundEnabled` | boolean | — | 是否开启声音 |

---

## 11. AMAS 自适应引擎模块

### 11.1 上报学习事件

**接口：** `POST /api/amas/process-event`
**说明：** 上报一条学习事件并触发 AMAS 状态更新（**不写入** `learning_records` 表）。应用层应优先调用 `POST /api/records`。

| 字段 | 类型 | 必填 | 约束/说明 |
|---|---|---|---|
| `wordId` | string | ✓ | 已存在的单词 ID |
| `isCorrect` | boolean | ✓ | 本次答题是否正确 |
| `responseTime` | number | ✓ | 响应时间（毫秒）；亦可使用 `response_time` snake_case |
| `sessionId` | string | — | 所属学习会话 ID |
| `isQuit` | boolean | — | 是否中途退出；默认 `false` |
| `dwellTime` | number | — | 停留时长（毫秒） |
| `pauseCount` | number | — | 暂停次数 |
| `switchCount` | number | — | 切换次数 |
| `retryCount` | number | — | 重试次数 |
| `focusLossDuration` | number | — | 失去焦点累计时长（毫秒） |
| `interactionDensity` | number | — | 交互密度 |
| `pausedTimeMs` | number | — | 暂停累计时长（毫秒） |
| `hintUsed` | boolean | — | 是否使用提示；默认 `false` |
| `confusedWith` | string | — | 易混淆的单词 ID，用于 IAD 混淆词对追踪 |

```json
{
  "wordId": "<id>",
  "isCorrect": true,
  "responseTime": 1850,
  "sessionId": "sess-uuid-here",
  "isQuit": false,
  "dwellTime": 3200,
  "hintUsed": false
}
```

**响应要点：** 返回 `ProcessResult`，包含 `sessionId`、`strategy`、`explanation`、`state`、`wordMastery`、`reward`、`coldStartPhase`。

---

### 11.2 批量上报学习事件

**接口：** `POST /api/amas/batch-process`
**说明：** 批量上报学习事件，最大条数受 `max_batch_size` 限制（默认 500）。

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `events` | array | ✓ | 数组元素结构同 11.1；最大 500 条 |

---

### 11.3 重置 AMAS 状态

**接口：** `POST /api/amas/reset`
**说明：** 重置当前用户的 AMAS 状态（恢复到默认值）。

**请求体：** 空

---

### 11.4 上报视觉疲劳分数

**接口：** `POST /api/amas/visual-fatigue`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `score` | number | ✓ | 视觉疲劳分数，范围 [0, 100] |

```json
{
  "score": 45.0
}
```

---

## 12. 遥测模块

### 12.1 提交遥测数据

**接口：** `POST /api/telemetry`
**必须请求头：** `x-device-id: <deviceId>`（缺失返回 400 `MISSING_DEVICE_ID`）
**请求体上限：** 64 KB
**响应 200：** `{ "success": true, "data": { "id": "<服务端生成的 telemetry UUID>" } }`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `eventType` | string | ✓ | `"session_start"` / `"periodic"` / `"on_demand"` |
| `requestId` | string | 条件必填 | `eventType` 为 `"on_demand"` 时必须提供，**值取自 SSE `telemetry_request` 事件的 `requestId`**；缺失返回 400 `INVALID_TELEMETRY` |
| `clientTs` | string | ✓ | 客户端时间，ISO 8601 UTC |
| `payload` | object | ✓ | 见下方字段说明；即使没数据也要传 `{}`。`eventType` 为 `"session_start"` 时通常携带 `payload.device` |

**payload 字段说明：**

*设备信息（`payload.device`，可选）：*

| 子字段 | 类型 | 约束 |
|---|---|---|
| `cpuCores` | number | 整数 |
| `memoryGb` | number | 浮点 |
| `screenWidth` | number | 整数，像素 |
| `screenHeight` | number | 整数，像素 |
| `pixelRatio` | number | 浮点，如 2.0 |
| `osName` | string | 如 `"macOS"` / `"Windows"` / `"iOS"` / `"Android"` |
| `browserName` | string | 如 `"Chrome"` / `"Safari"` |
| `browserVersion` | string | 版本号字符串 |
| `timezone` | string | IANA 时区，如 `"Asia/Shanghai"` |
| `language` | string | 语言代码，如 `"zh-CN"` |
| `touchSupport` | boolean | — |
| `onlineStatus` | boolean | — |

*会话指标（`payload` 内部顶层字段，可选；负数返回 422 `INVALID_PAYLOAD`）：*

| 字段 | 类型 | 约束 |
|---|---|---|
| `sessionDurationSecs` | number | 整数 ≥ 0；**单调累计本次 session 的总时长**（不是单次窗口时长），服务端按 SUM 聚合到当日 `study_duration_secs`，每次 `periodic` 上报传当前累计值即可 |
| `actionsPerMin` | number | 浮点 ≥ 0 |
| `errorCount` | number | 整数 ≥ 0 |
| `avgResponseTimeMs` | number | 浮点 ≥ 0 |

*行为追踪（`payload.behavior`，可选）：*

| 子字段 | 类型 |
|---|---|
| `currentRoute` | string |
| `clickCount` | number |
| `clickTargets` | array，元素为 `{ "label": string, "tag": string }` |
| `scrollDepthPct` | number (0–100) |
| `visibilityChanges` | number |
| `routeChanges` | number |

*功能使用（`payload.featureUsage`，可选）：*
自由 JSON 对象，键为功能名称，值为使用次数（number）。

**完整示例：**
```json
{
  "eventType": "periodic",
  "requestId": null,
  "clientTs": "2024-01-15T08:30:00Z",
  "payload": {
    "device": {
      "cpuCores": 8,
      "memoryGb": 16.0,
      "screenWidth": 390,
      "screenHeight": 844,
      "pixelRatio": 3.0,
      "osName": "iOS",
      "browserName": "Safari",
      "browserVersion": "17.2",
      "timezone": "Asia/Shanghai",
      "language": "zh-CN",
      "touchSupport": true,
      "onlineStatus": true
    },
    "sessionDurationSecs": 600,
    "actionsPerMin": 8.5,
    "errorCount": 0,
    "avgResponseTimeMs": 1350.0,
    "behavior": {
      "currentRoute": "/learning",
      "clickCount": 85,
      "clickTargets": [
        { "label": "Submit", "tag": "button" }
      ],
      "scrollDepthPct": 60,
      "visibilityChanges": 1,
      "routeChanges": 4
    },
    "featureUsage": {
      "flashcard": 30,
      "etymology": 2,
      "options": 28
    }
  }
}
```

---

### 12.2 心跳节奏与 SSE 联动

`POST /api/telemetry` 除了写入历史数据，还会刷新该 device 的心跳时间戳。服务端 watchdog 每 5 秒扫描一次，**任一活跃 SSE 设备超过 10 秒未到包计 1 次 miss，连续 5 次 miss 推送 SSE `data_corrupted` 事件**让客户端走重拉数据流程。

由此对客户端的硬性要求：

| 场景 | 上报要求 |
|---|---|
| 维持 SSE 长连期间（默认） | 必须以 **≤ 10 秒**周期上报 `eventType: "periodic"`，**建议 5–8 秒**留网络抖动余量 |
| 收到 SSE `telemetry_request` 事件 | **立即**上报 `eventType: "on_demand"`，`requestId` 字段填该事件携带的 UUID |
| App 启动 / 前台恢复 | 上报 `eventType: "session_start"`，`payload.device` 携带硬件/环境信息 |
| App 进入后台或断开 SSE | 停止 periodic；watchdog 30 秒内会清掉该 device 的心跳记录，恢复时重新走 `session_start` |

**注意：** 没有活跃 SSE 连接的客户端不参与心跳判断，可以低频或不上报；但仅对 `eventType: "session_start"` / `"periodic"` 也仍需正常上报，作为历史数据落库。

### 12.3 上报策略示例（iOS）

```
[App 启动 / 前台恢复]
   └── POST /api/telemetry  { eventType: "session_start", payload.device: {...} }
   └── 启动 5–8 秒 timer

[timer 触发]
   └── 收集本次 session 累计指标快照（sessionDurationSecs 单调累加，不重置）
   └── POST /api/telemetry  { eventType: "periodic", payload: {...} }

[SSE 推送 telemetry_request]
   └── 立即 POST /api/telemetry  { eventType: "on_demand", requestId: <SSE 事件中的值>, payload: {...} }

[App 进入后台]
   └── 停止 timer，断开 SSE
```

> **不要**在恢复后补发崩溃期间未发出的 telemetry：服务端按 `clientTs` 入库，但 `data_corrupted` SSE 已经触发过了，应直接走 `session_start` 重新建立心跳。

### 12.4 资源包安装上报

资源包热更安装 / 校验 / 回滚结束后上报结果，供 admin 统计聚合。与提交遥测同源鉴权（登录态特性）。

**接口：** `POST /api/telemetry/resource-pack-install`
**认证：** Bearer Token + `x-device-id` 请求头（`x-device-id` 可选；缺失则该条不关联设备）
**响应 200：** `{ "success": true, "data": { "received": true } }`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `packId` | string | ✓ | 资源包业务 ID |
| `version` | string | ✓ | 本次安装 / 校验 / 回滚涉及的版本 |
| `outcome` | string | ✓ | `"installed"`（安装成功）/ `"verify_failed"`（签名或哈希校验失败）/ `"rollback"`（回滚到上一版本）/ `"download_failed"`（下载失败，m070 起）/ `"apply_failed"`（本地解析或落盘失败，m070 起） |
| `appVersion` | string | — | 客户端 App 版本，可选 |

**示例：**
```json
{
  "packId": "wordbook-core",
  "version": "1.2.3",
  "outcome": "installed",
  "appVersion": "1.0.0"
}
```

> 此上报不参与心跳判断，也不受遥测限流 / 采样影响，是即发即忘的安装结果日志。每个 `(version, outcome)` 由 admin 端 `GET /api/admin/resource-packs/:packId/stats` 聚合计数。

### 12.5 埋点事件流（app-events，m073）

三端统一的**命名事件**埋点通道，与 §12.1 的心跳/摘要遥测分工：那边是周期快照（eventType 三值枚举 + summary 投影），这边是离散行为/错误/性能事件（`name` + `props`）。**本流与学习记录管线完全隔离**——`clientEventId` 只做本流去重，与 `/records` 的 `clientRecordId` 无任何关联（sessionEvents 的 `clientEventId === clientRecordId` 不变式不适用于此）。

**接口：** `POST /api/telemetry/app-events`
**认证：** Bearer Token + `x-device-id` + `x-device-platform` + `x-app-version` 三头必填；设备须已注册且归属一致（403 `DEVICE_NOT_REGISTERED` / `DEVICE_OWNERSHIP_MISMATCH`）。**不做 device upsert、不刷心跳**。

**请求体：** `{ "events": [ ... ] }`，批量上限 **50 条**（超限整批 400 `APP_EVENTS_TOO_LARGE`）。逐条字段：

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `clientEventId` | string | ✓ | ≤128 字符，客户端生成 UUID；`(deviceId, clientEventId)` 跨请求幂等去重（重放计入 `duplicates`） |
| `name` | string | ✓ | `^[a-z0-9_]{1,64}$`（词表见下）；name 是聚合键，自由文本一律放 `props` |
| `category` | string | ✓ | `behavior` \| `error` \| `perf` |
| `clientTsMs` | number | ✓ | 服务器对时毫秒；钳制窗 `[now-7d, now+5min]`——**比 learning 事件的 30min 宽是有意为之**（离线队列可滞留数天，勿"统一"） |
| `props` | object | — | ≤16 键、值仅标量（字符串 ≤128 字符）、序列化 ≤2KB；违规逐条拒 `APP_EVENT_INVALID_PROPS` |

**响应 200：** `{ "accepted", "duplicates", "sampledOut", "failed", "errors": [{ "index", "clientEventId", "code", "message" }] }`——逐条部分成功，绝不整批 400。限流命中软丢弃 `{ "accepted": 0, "throttled": true }`（独立 `ae:` 配额，不与心跳共享）。

**采样：** `behavior` / `perf` 受 `probe_sampling_config` 的 `app_behavior` / `app_perf` 行控制（确定性哈希，逐条）；`error` 恒不采样。被采样丢弃计入 `sampledOut`，客户端视为已送达。

**事件词表（三端统一 snake_case，props 键固定）：**

| category | name | props |
|---|---|---|
| behavior | `session_start` / `session_end` | `{launchType?}` / `{durationSec}` |
| behavior | `screen_view` | `{screen}`：`home/study/review/wordbook/word_detail/profile/settings/stats/login/onboarding/notifications/feedback`（`review` 为移动端复习主 Tab；词表外路由不上报——screen 是聚合维度，不收自由串。跨端归并口径：快速练习等练习类二级入口一律并入 `study`，词库中心/词库详情并入 `wordbook`，历史/会话列表并入 `stats`） |
| behavior | `study_start` / `study_complete` / `study_quit` | `{wordCount?}` |
| behavior | `word_lookup` / `word_favorite` / `word_note` / `wordbook_switch` / `feedback_submit` | — |
| behavior | `pack_update` | `{outcome}` |
| error | `api_failure` | `{endpoint（模板化路径）, status, code?}` |
| error | `crash` | `{signature（sha256 前 16）, kind}` |
| error | `app_error` | `{signature, kind}` |
| perf | `app_launch` | `{ms}` |
| perf | `page_load` | `{screen, ms}` |
| perf | `api_rtt` | `{endpoint, ms, status}`（客户端 1/10 采样） |

> 服务端存储：raw 表 `app_events` 90 天 retention；日聚合（`app_event_daily`/`app_user_daily`/`app_user_first_seen`）由每日 01:10 的 `app_event_rollup` worker 重算最近 7 天（与钳制窗一致），永久保留。分析读端点 `GET /api/admin/app-events/{overview|trend|top-events|errors|perf|funnel|retention-matrix|activity}`。

---

## 13. 单词管理模块（Admin）

### 13.1 创建单词

**接口：** `POST /api/words`（需 Admin Token）

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `id` | string \| null | — | 自定义 ID；若为 null 则服务端自动生成 UUID |
| `text` | string | ✓ | 去除首尾空格后非空 |
| `meaning` | string | ✓ | 去除首尾空格后非空 |
| `pronunciation` | string \| null | — | 音标，如 `"/əˈbændən/"` |
| `partOfSpeech` | string \| null | — | 词性，如 `"noun"` / `"verb"` |
| `difficulty` | number \| null | — | 难度，范围 [0.0, 1.0]；默认 0.5 |
| `examples` | string[] \| null | — | 例句数组 |
| `tags` | string[] \| null | — | 标签数组，如 `["GRE", "common"]` |

---

### 13.2 批量创建单词

**接口：** `POST /api/words/batch`（需 Admin Token）

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `words` | array | ✓ | 最大 500 条；元素结构同 13.1 |

---

### 13.3 批量按 ID 获取单词

**接口：** `POST /api/words/batch-get`（需 Auth Token）

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `ids` | string[] | ✓ | 最大 500 条 |

```json
{
  "ids": ["<id1>", "<id2>", "<id3>"]
}
```

---

### 13.4 从 URL 导入单词

**接口：** `POST /api/words/import-url`（需 Admin Token）

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `url` | string | ✓ | 公网 HTTP/HTTPS；禁止 localhost/内网；超时 30 秒；最大 10 MB |

文件内容格式（UTF-8 文本）：
- 每行一个单词：`word\tmeaning`（制表符）或 `word - meaning`（破折号+空格）
- `#` 开头为注释行，忽略
- 空行忽略

---

## 14. 内容管理模块（Admin）

### 14.1 设置单词词素

**接口：** `POST /api/content/morphemes/:wordId`（需 Admin Token）

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `morphemes` | array | ✓ | 词素数组，见下表 |

`morphemes` 数组元素：

| 子字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `text` | string | ✓ | 词素文本，如 `"un-"` |
| `type` | string | ✓ | 推荐 `"prefix"` / `"root"` / `"suffix"`；当前服务端不做枚举校验 |
| `meaning` | string | ✓ | 词素含义 |

```json
{
  "morphemes": [
    { "text": "un-", "type": "prefix", "meaning": "不、非" },
    { "text": "forgiv", "type": "root", "meaning": "宽恕" },
    { "text": "-able", "type": "suffix", "meaning": "能够…的" }
  ]
}
```

---

## 15. 单词收藏模块

### 15.1 收藏单词

**接口：** `POST /api/word-favorites/:wordId`

**请求体：** 空（单词 ID 在路径中）

**说明：** 幂等——重复收藏返回既有记录。

---

### 15.2 取消收藏

**接口：** `DELETE /api/word-favorites/:wordId`

**请求体：** 空（单词 ID 在路径中）

**说明：** 幂等——未收藏的 word 调用返回 `deleted: false`。

> 写收藏请走本节两个端点，**不要**通过 `PUT /api/word-states/:wordId` 改 `bookmarked` 字段——后者只读。

---

## 16. 单词笔记模块

### 16.1 创建笔记

**接口：** `POST /api/word-notes`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `wordId` | string | ✓ | 已存在的单词 ID |
| `content` | string | ✓ | 去除首尾空格后非空，最长 5000 字符 |

```json
{
  "wordId": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "content": "记忆要点：源自拉丁文 ad+bandon"
}
```

**响应 201：** `WordNote` 对象（含 `id` / `createdAt` / `updatedAt`）

错误码：`NOT_FOUND`（单词不存在）/ `WORD_NOTE_EMPTY_CONTENT` / `WORD_NOTE_TOO_LONG`。

---

### 16.2 更新笔记

**接口：** `PUT /api/word-notes/:id`

| 字段 | 类型 | 必填 | 约束 |
|---|---|---|---|
| `content` | string | ✓ | 同 16.1 |

```json
{
  "content": "改后内容"
}
```

错误码：`NOT_FOUND`（笔记不存在或不属于当前用户）/ `WORD_NOTE_EMPTY_CONTENT` / `WORD_NOTE_TOO_LONG`。

---

### 16.3 删除笔记

**接口：** `DELETE /api/word-notes/:id`

**请求体：** 空（笔记 ID 在路径中）

错误码：`NOT_FOUND`（笔记不存在或不属于当前用户）。

---

## 附录：字段校验规则汇总

### 密码规则

- 最短 8 字符，最长 256 字符
- 必须同时包含：大写字母（A-Z）、小写字母（a-z）、数字（0-9）
- 不强制要求特殊字符

### 邮箱规则

- 遵循 RFC 5321
- 总长度 ≤ 254 字符
- 本地部分（`@` 前）≤ 64 字符，允许 `a-z A-Z 0-9 . _ + -`
- 域名部分必须包含 `.`，允许字母、数字、`-`、`.`

### 用户名规则

- 2–50 字符
- 允许：Unicode 字母数字（涵盖英文、中日韩等）、下划线 `_`、连字符 `-`、空格
- 不允许其他特殊字符（如 `.` `@` `!` 等标点）

### 时间格式

所有时间字段使用 ISO 8601 UTC 格式：`"2024-01-15T08:30:00Z"`

### 数值范围速查

| 字段 | 最小 | 最大 |
|---|---|---|
| `difficulty`（单词难度） | 0.0 | 1.0 |
| `masteryLevel`（掌握程度） | 0.0 | 1.0 |
| `recentPerformance`（准确率） | 0.0 | 1.0 |
| `dailyWordCount` | 1 | 200 |
| `dailyMasteryTarget` | 1 | 100 |
| `medianSessionLengthMins` | 1 | 480 |
| `sessionsPerDay` | 1 | 20 |
| `preferredHours` 元素 | 0 | 23 |
| 批量操作数组长度 | 1 | 500 |
| `excludeWordIds` 长度 | 0 | 1000 |
| 通知查询 `limit` | 1 | 200 |
| 语义搜索 `limit` | 1 | 50 |
| 到期单词 `limit` | 1 | 200 |
| `score`（视觉疲劳分数） | 0 | 100 |
| 笔记 `content` 字符数 | 1 | 5000 |
