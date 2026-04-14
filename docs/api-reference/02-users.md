# 用户接口

用户接口分为两组：

- **用户基础信息** (`/api/users`) — 个人资料查看/编辑、密码修改、学习统计
- **用户画像** (`/api/user-profile`) — 奖励偏好、认知画像、学习风格、作息类型、习惯画像、头像上传

所有接口均需要用户认证（`Authorization: Bearer <token>`）。

---

## 用户基础信息

基础路径: `/api/users`

### GET /api/users/me

**描述**: 获取当前登录用户的个人资料。

**认证**: 需要

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| id | string | 用户 ID |
| email | string | 邮箱 |
| username | string | 用户名 |
| isBanned | boolean | 是否被封禁 |

---

### PUT /api/users/me

**描述**: 更新当前用户的个人资料。

**认证**: 需要

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| username | string | 否 | 新用户名，需符合用户名校验规则 |

**响应体** (200 OK): 同 `GET /api/users/me`。

**错误码**:
- `USER_INVALID_USERNAME` — 用户名不符合要求

---

### PUT /api/users/me/password

**描述**: 修改当前用户的密码。成功后该用户所有会话将被撤销，需重新登录。

**认证**: 需要

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| currentPassword | string | 是 | 当前密码 |
| newPassword | string | 是 | 新密码，需符合强度校验 |

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| passwordChanged | boolean | 固定为 `true` |

**错误码**:
- `AUTH_WEAK_PASSWORD` — 新密码强度不足
- `401` — 当前密码不正确

---

### GET /api/users/me/stats

**描述**: 获取当前用户的学习统计数据。

**认证**: 需要

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| totalWordsLearned | number | 已学习单词总数（去重） |
| totalSessions | number | 学习会话总数 |
| totalRecords | number | 学习记录总数 |
| streakDays | number | 连续学习天数 |
| accuracyRate | number | 正确率（0.0 ~ 1.0） |

---

## 用户画像

基础路径: `/api/user-profile`

### GET /api/user-profile/reward

**描述**: 获取当前用户的奖励偏好设置。

**认证**: 需要

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| rewardType | string | 奖励类型，默认 `"standard"` |

---

### PUT /api/user-profile/reward

**描述**: 设置用户的奖励偏好类型。

**认证**: 需要

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| rewardType | string | 是 | 可选值: `standard`, `explorer`, `achiever`, `social` |

**响应体** (200 OK): 同 `GET /api/user-profile/reward`。

**错误码**:
- `INVALID_REWARD_TYPE` — 奖励类型不在允许的列表中

---

### GET /api/user-profile/cognitive

**描述**: 获取用户的认知画像，由 AMAS 系统根据学习行为自动计算。

**认证**: 需要

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| memoryCapacity | number | 记忆容量（0.0 ~ 1.0），默认 0.5 |
| processingSpeed | number | 处理速度（0.0 ~ 1.0），默认 0.5 |
| stability | number | 稳定性（0.0 ~ 1.0），默认 0.5 |

---

### GET /api/user-profile/learning-style

**描述**: 获取用户的学习风格概要，从认知画像中提取的核心指标。

**认证**: 需要

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| processingSpeed | number | 处理速度 |
| memoryCapacity | number | 记忆容量 |
| stability | number | 稳定性 |

---

### GET /api/user-profile/chronotype

**描述**: 获取用户的作息类型，根据偏好学习时段自动推断。

**认证**: 需要

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| chronotype | string | 作息类型: `"morning"`（偏好时段 < 10 点）、`"evening"`（> 20 点）或 `"neutral"` |
| preferredHours | number[] | 偏好学习时段（0-23 的小时数组） |

---

### GET /api/user-profile/habit

**描述**: 获取用户的习惯画像。优先返回用户手动设置的值，未设置时返回 AMAS 系统计算值。

**认证**: 需要

**请求体**: 无

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| preferred_hours | number[] | 偏好学习时段 |
| median_session_length_mins | number | 单次学习时长中位数（分钟） |
| sessions_per_day | number | 每日学习次数 |
| temporal_performance | object | 时段表现统计（仅 AMAS 计算值包含） |

---

### POST /api/user-profile/habit

**描述**: 手动设置用户的习惯画像参数。

**认证**: 需要

**请求体**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| preferredHours | number[] | 否 | 偏好学习时段，每个值 0-23 |
| medianSessionLengthMins | number | 否 | 单次学习时长（分钟），1-480 |
| sessionsPerDay | number | 否 | 每日学习次数，1-20 |

**响应体** (200 OK): 存储后的习惯画像对象（使用 snake_case 键名）。

**错误码**:
- `INVALID_PREFERRED_HOURS` — 偏好时段的值必须在 0 到 23 之间
- `INVALID_SESSIONS_PER_DAY` — 每日学习次数必须在 1 到 20 之间
- `INVALID_SESSION_LENGTH` — 单次学习时长必须在 1 到 480 分钟之间

---

### POST /api/user-profile/avatar

**描述**: 上传用户头像。请求体为图片二进制数据（非 multipart 表单）。

**认证**: 需要

**Content-Type**: `application/octet-stream` 或对应图片 MIME 类型

**请求体**: 图片二进制数据，通过文件头魔数检测格式。

**约束**:
- 最大文件大小: 512 KB
- 支持格式: PNG、JPEG、GIF、WebP

**响应体** (200 OK):

| 字段 | 类型 | 说明 |
|------|------|------|
| avatarUrl | string | 头像访问路径，如 `/avatars/{userId}.png` |

**错误码**:
- `AVATAR_EMPTY` — 未上传文件
- `AVATAR_TOO_LARGE` — 文件大小超过 512 KB
- `AVATAR_INVALID_TYPE` — 不支持的图片格式
