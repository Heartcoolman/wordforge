# 基础设施接口

本节涵盖健康检查、状态查询、实时事件推送和遥测上报四个模块。

## 路由前缀

| 模块 | 前缀 | 维护模式豁免 |
|------|------|:------------:|
| 健康检查 | `/health` （不在 `/api` 下） | 是（不经过维护中间件） |
| 状态查询 | `/api/status` | 是 |
| 实时事件 | `/api/realtime` | 是 |
| 遥测上报 | `/api/telemetry` | 是 |

---

## 健康检查

### GET /health/

基础健康检查，返回服务运行状态和启动时长。

| 项目 | 说明 |
|------|------|
| 认证 | 无需认证 |

**响应体**

```json
{
  "status": "ok",
  "uptimeSecs": 3600,
  "store": {
    "healthy": true
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | `string` | 固定 `"ok"` |
| `uptimeSecs` | `u64` | 服务启动至今的秒数 |
| `store.healthy` | `bool` | 固定 `true` |

---

### GET /health/live

存活探针，仅返回 HTTP 状态码，无响应体。

| 项目 | 说明 |
|------|------|
| 认证 | 无需认证 |
| 成功状态码 | `200 OK` |

---

### GET /health/ready

就绪探针，向数据库发送探测查询确认可用性。

| 项目 | 说明 |
|------|------|
| 认证 | 无需认证 |
| 成功状态码 | `200 OK` |
| 失败状态码 | `503 Service Unavailable` |

---

### GET /health/database

数据库健康详情，包含查询延迟。

| 项目 | 说明 |
|------|------|
| 认证 | 管理员 (`AdminAuthUser`) |

**响应体**

```json
{
  "healthy": true,
  "latencyUs": 152,
  "consecutiveFailures": 0
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `healthy` | `bool` | 数据库是否可用 |
| `latencyUs` | `u64` | 探测查询延迟（微秒） |
| `consecutiveFailures` | `u64` | 连续失败次数（`0` 或 `1`） |

---

### GET /health/metrics

AMAS 算法指标快照。

| 项目 | 说明 |
|------|------|
| 认证 | 管理员 (`AdminAuthUser`) |

**响应体**

```json
{
  "algorithms": { ... }
}
```

`algorithms` 内容由 `AMASEngine::metrics_registry().snapshot()` 动态生成。

---

## 状态查询

### GET /api/status/

查询服务当前状态（维护模式、版本号）。

| 项目 | 说明 |
|------|------|
| 认证 | 无需认证（但需通过设备中间件） |

**响应体**

```json
{
  "success": true,
  "data": {
    "maintenanceMode": false,
    "version": "0.1.0-abc1234"
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `maintenanceMode` | `bool` | 是否处于维护模式 |
| `version` | `string` | 编译时注入的 Git 版本号 |

---

### GET /api/status/device-ban

查询指定设备是否被封禁。

| 项目 | 说明 |
|------|------|
| 认证 | 无需认证（但需通过设备中间件） |

**查询参数**

| 参数 | 类型 | 必填 | 说明 |
|------|------|:----:|------|
| `deviceId` | `string` | 是 | 设备 ID |

**响应体**

```json
{
  "success": true,
  "data": {
    "banned": false
  }
}
```

---

## 实时事件（SSE）

### GET /api/realtime/events

建立 SSE 长连接，服务端实时推送事件。

| 项目 | 说明 |
|------|------|
| 认证 | 用户认证 (`AuthUser`) |
| 协议 | Server-Sent Events |
| Keep-Alive | 每 15 秒发送 `keepalive` 心跳 |
| 并发限制 | 全局最大连接数由 `config.limits.max_sse_connections` 控制，超出返回 `429` |

**请求头**

| Header | 必填 | 说明 |
|--------|:----:|------|
| `X-Device-Id` | 否 | 设备标识，用于服务端追踪连接 |
| `X-Device-Platform` | 否 | 平台标识，缺省为 `"unknown"` |

**事件类型**

#### `amas_state`

AMAS 状态变化，每 5 秒轮询一次，仅在 `totalEventCount` 变化时推送。

```json
{
  "type": "state_change",
  "attention": 0.8,
  "fatigue": 0.2,
  "motivation": 0.7,
  "confidence": 0.6,
  "sessionEventCount": 12,
  "totalEventCount": 345
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `attention` | `f64` | 注意力值 |
| `fatigue` | `f64` | 疲劳度 |
| `motivation` | `f64` | 动机值 |
| `confidence` | `f64` | 自信度 |
| `sessionEventCount` | `u64` | 当前会话事件数 |
| `totalEventCount` | `u64` | 累计事件数 |

#### `maintenance`

维护模式状态广播（来自全局广播通道）。

```json
{ "type": "maintenance", "active": true }
```

#### `update_available`

新版本更新通知（由管理员触发广播）。

```json
{ "version": "1.2.0", "message": "新增XX功能" }
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `version` | `string` | 新版本号 |
| `message` | `string` | 更新说明 |

#### `banned`

当前连接对应的设备被封禁（通过定向通道推送）。

```json
{ "type": "banned" }
```

#### `unbanned`

当前连接对应的设备被解封。

```json
{ "type": "unbanned" }
```

#### `telemetry_request`

服务端主动请求客户端上报遥测数据。

```json
{ "type": "telemetry_request", "requestId": "uuid" }
```

#### 定向事件（`maintenance` 通道变体）

通过每连接独立通道推送的维护事件，与全局广播的 `maintenance` 事件格式相同。

```json
{ "type": "maintenance", "active": true }
```

---

## 遥测上报

### POST /api/telemetry/

提交客户端遥测数据。请求体上限 64 KiB。

| 项目 | 说明 |
|------|------|
| 认证 | 用户认证 (`AuthUser`) |

**请求头**

| Header | 必填 | 说明 |
|--------|:----:|------|
| `X-Device-Id` | 是 | 设备标识，缺失返回 `400 MISSING_DEVICE_ID` |

**请求体**

```json
{
  "eventType": "periodic",
  "requestId": "uuid",
  "clientTs": "2024-01-01T00:00:00Z",
  "payload": { ... }
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|:----:|------|
| `eventType` | `string` | 是 | 事件类型（如 `"periodic"`、`"on_demand"`） |
| `requestId` | `string` | 条件 | 当 `eventType` 为 `"on_demand"` 时必填 |
| `clientTs` | `string` | 是 | 客户端时间戳 |
| `payload` | `object` | 是 | 自由格式负载 |

**`payload` 校验规则**

| 字段 | 类型 | 约束 |
|------|------|------|
| `sessionDurationSecs` | `i64` | 不能为负数 |
| `errorCount` | `i64` | 不能为负数 |
| `actionsPerMin` | `f64` | 不能为负数 |
| `avgResponseTimeMs` | `f64` | 不能为负数 |

违反约束返回 `422 INVALID_PAYLOAD`。

**响应体**

```json
{
  "success": true,
  "data": {
    "id": "uuid"
  }
}
```
