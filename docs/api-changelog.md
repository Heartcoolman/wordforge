# API 变更日志（客户端对接参考）

**日期**：2026-04-17  
**适用版本**：当前 `dev` 分支  
**面向**：客户端开发团队

---

## 变更概览

| 模块 | 变更类型 | 涉及端点数 |
|---|---|---|
| 认证（Auth） | **破坏性** | 3 |
| 管理分析（Admin Analytics） | 新增 + 修改 | 4 |
| 管理统计（Admin Stats） | 修改 | 1 |
| 健康检查（Health） | **结构变更** | 1 |
| 学习（Learning） | 行为修正 | 2 |
| 词状态（Word States） | 行为修正 | 1 |
| 用户画像（User Profile） | 新增字段 | 1 |

---

## 1. 认证模块（Breaking）

### `POST /api/auth/login` · `POST /api/auth/register` · `POST /api/auth/refresh`

**变更类型**：破坏性（Cookie 策略改变）

#### 1.1 Cookie SameSite 策略变更

| 字段 | 旧值 | 新值 |
|---|---|---|
| `SameSite` | `Strict` | `None` |

**影响**：客户端在跨域场景（前端域与 API 域不同）下现在可以正常接收和发送 Cookie。如果客户端之前依赖 `Strict` 语义（例如有防 CSRF 的额外假设），需重新评估。

> Cookie 的 `Secure` 标志由服务端环境变量 `COOKIE_SECURE` 控制（默认 `false`），HTTP 和 HTTPS 均可正常工作。

#### 1.2 Cookie Max-Age 改为服务端配置驱动

- `token` Cookie 的 `Max-Age` 现在等于服务端 `jwt_expires_in_hours × 3600`
- `refresh_token` Cookie 的 `Max-Age` 现在等于服务端 `refresh_token_expires_in_hours × 3600`

客户端不应硬编码 Cookie 有效期，应以 Cookie 本身的 `Max-Age` 或 `Expires` 属性为准。

#### 1.3 登录逻辑顺序变更（`POST /api/auth/login`）

封禁/锁定校验现在在密码校验**之前**执行。行为变化：

- 被封禁账户登录时，将直接返回封禁错误，不再进行密码比对
- 防止通过响应时间差推断账户是否存在（时序攻击防护）

**客户端无需改动**，仅需了解错误返回顺序可能不同。

---

## 2. 管理分析模块（Admin Analytics）

> 以下端点仅限管理端（需 admin token）。

### 新增：`GET /api/admin/analytics/daily-active-users`

**描述**：返回最近 N 天的每日活跃用户数时序数据

**Query 参数**：

| 参数 | 类型 | 默认值 | 范围 |
|---|---|---|---|
| `days` | integer | `7` | 1–30 |

**Response**：

```json
[
  { "date": "2026-04-17", "count": 42 },
  { "date": "2026-04-16", "count": 38 }
]
```

---

### 新增：`GET /api/admin/analytics/daily-records`

**描述**：返回最近 N 天的每日记录提交量（含正确/总数）

**Query 参数**：同上（`days`，1–30，默认 7）

**Response**：

```json
[
  { "date": "2026-04-17", "total": 320, "correct": 251 },
  { "date": "2026-04-16", "total": 290, "correct": 218 }
]
```

---

### 修改：`GET /api/admin/analytics/engagement`

Response 新增 `trend` 字段：

```json
{
  "activeToday": 42,
  "trend": {
    "activeToday": 10.5
  }
}
```

`trend.activeToday`：相对昨日的百分比变化（正数为增长，负数为下降）

---

### 修改：`GET /api/admin/analytics/learning`

Response 新增 `trend` 字段：

```json
{
  "totalRecords": 15000,
  "overallAccuracy": 0.82,
  "trend": {
    "totalRecords": 5.2,
    "overallAccuracy": -1.1
  }
}
```

---

## 3. 管理统计（Admin Stats）

### 修改：`GET /api/admin/stats`

Response 新增顶层 `trend` 字段：

```json
{
  "totalUsers": 500,
  "totalRecords": 15000,
  "trend": {
    "users": 2.0,
    "records": 5.2
  }
}
```

`trend` 中的值均为相对昨日的百分比变化。

---

## 4. 健康检查（Health）

### 修改：`GET /health`（结构破坏性变更）

**旧 Response**：

```json
{ "status": "ok" }
```

**新 Response**：

```json
{
  "status": "ok",
  "uptimeSecs": 3600,
  "services": {
    "store": "ok",
    "amas": "ok",
    "sse": "ok",
    "wordbookCenter": "ok"
  }
}
```

| 字段 | 说明 |
|---|---|
| `status` | `"ok"` \| `"degraded"` \| `"down"` |
| `uptimeSecs` | 服务运行时长（秒） |
| `services.store` | 数据库连通性 |
| `services.amas` | AMAS 算法引擎状态 |
| `services.sse` | SSE 实时事件服务状态 |
| `services.wordbookCenter` | 词书中心远程连通性 |

**客户端迁移**：如果客户端有轮询 `/health` 并解析 `status` 字段的逻辑，格式本身兼容，但建议同时消费 `services` 字段以实现更细粒度的故障检测。

---

## 5. 学习模块（行为修正）

### 修改：`POST /api/learning/complete-session`

**变更**：精度计算修正

- 旧逻辑：使用 `totalQuestions`（计划题数）作为分母
- 新逻辑：使用实际 session 中的记录数作为分母

**影响**：返回的 `accuracy` 字段数值可能与之前不同，现在更准确地反映实际答题情况。

---

### 修改：`POST /api/learning/generate-options`

**变更**：选项生成回退策略改进

- 当同义词/干扰词数量不足时，从全词库补充真实词条
- 不再出现 `"(未知)"` `"(无释义)"` 等占位符字符串
- 最终兜底填充改为 `"—"`（em dash）

**影响**：选项质量提升，客户端不再需要特殊处理占位符。

---

## 6. 词状态模块（行为修正）

### 修改：`POST /api/word-states/batch-update`

**变更**：批量更新现在在单个数据库事务内执行

- 旧行为：逐条更新，部分失败不回滚
- 新行为：全部成功才提交，任意失败则全部回滚

**影响**：原子性保证增强。客户端如果有重试逻辑，需确保幂等性（相同的 `wordId` 重复提交是安全的）。

---

## 7. 用户画像模块（新增字段）

### 修改：`GET /api/user-profile/habit`

Response 新增可选字段 `temporalPerformance`：

```json
{
  "preferredHours": [9, 10, 21],
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

`temporalPerformance` 为可选字段，无数据时为 `null`。`hourlyStats` 数组长度为 24（对应 0–23 时），无数据的小时项中数值均为 0。

---

## 迁移检查清单

- [ ] Cookie 处理逻辑已支持 `SameSite=None`
- [ ] 不硬编码 Cookie 有效期，改为读取 `Max-Age` 属性
- [ ] `/health` 解析代码已适配新 response 结构
- [ ] admin 看板已接入 `daily-active-users` 和 `daily-records` 端点
- [ ] 已消费各 analytics 端点 response 中的 `trend` 字段
- [ ] 已知悉 `complete-session` 精度计算变化
- [ ] 已知悉 `batch-update` 的原子性保证
