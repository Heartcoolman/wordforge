# WordForge API 变更通知 — iOS 客户端对接指南

## 一、新增请求头

所有 `/api/*` 请求（`/api/admin/*` 除外）需携带以下请求头：

| 请求头 | 是否必需 | 说明 |
|---|---|---|
| `X-Device-Id` | 是 | 设备唯一标识，建议使用 `identifierForVendor` 或自行生成并持久化的 UUID |
| `X-Device-Platform` | 建议 | 填 `ios`，缺省值为 `unknown` |

> SSE 连接（`/api/realtime/events`）同样需要携带上述请求头。

---

## 二、新增接口

### 1. 服务状态查询

```
GET /api/status
```

- 认证：无
- 用途：启动时检查维护状态和服务端版本

**响应 200：**

```json
{
  "maintenanceMode": false,
  "version": "0.5.0"
}
```

### 2. 遥测数据上报

```
POST /api/telemetry
```

- 认证：用户 JWT
- 必需请求头：`X-Device-Id`
- Body 大小限制：64 KB

**请求体：**

```json
{
  "eventType": "heartbeat",
  "requestId": null,
  "clientTs": "2026-04-11T10:00:00Z",
  "payload": {
    "sessionDurationSecs": 120,
    "errorCount": 0,
    "actionsPerMin": 5.2,
    "avgResponseTimeMs": 230.5
  }
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `eventType` | string | 事件类型，如 `heartbeat`、`on_demand` |
| `requestId` | string? | 可选；`eventType` 为 `on_demand` 时**必填**（值来自 SSE `telemetry_request` 事件） |
| `clientTs` | string | 客户端时间戳（ISO 8601） |
| `payload` | object | 自由结构，其中 `sessionDurationSecs`/`errorCount` 不可为负整数，`actionsPerMin`/`avgResponseTimeMs` 不可为负浮点 |

**响应 200：**

```json
{ "id": "generated-uuid" }
```

**错误：**

| 状态码 | code | 触发条件 |
|---|---|---|
| 400 | `MISSING_DEVICE_ID` | 缺少 `X-Device-Id` 请求头 |
| 400 | `INVALID_TELEMETRY` | `on_demand` 事件缺少 `requestId` |
| 422 | `INVALID_PAYLOAD` | payload 中数值字段为负数 |

---

## 三、SSE 新增事件

SSE 通道 `GET /api/realtime/events` 新增以下三种事件：

### `maintenance` — 维护模式变更

```json
{ "type": "maintenance", "active": true }
```

客户端收到后应根据 `active` 值展示或关闭维护提示页面。

### `update_available` — 版本更新推送

```json
{ "version": "1.2.0", "message": "有新版本可用，请刷新页面获取最新内容" }
```

客户端收到后可弹出更新提示。

### `telemetry_request` — 服务端请求遥测上报

```json
{ "type": "telemetry_request", "requestId": "uuid" }
```

客户端收到后应采集当前状态数据，调用 `POST /api/telemetry`，`eventType` 填 `on_demand`，`requestId` 填此处的值。

---

## 四、新增错误响应

### 设备封禁 — 403

任何请求可能返回（`X-Device-Id` 对应的设备被管理员封禁时）：

```json
{
  "code": "CLIENT_BANNED",
  "message": "设备已被封禁"
}
```

客户端应中止请求并提示用户。

### 维护模式 — 503

维护模式开启期间，除以下路径外所有接口返回 503：

- `GET /api/status`
- `GET /api/realtime/events`
- `POST /api/telemetry`
- `/health`

```json
{
  "code": "MAINTENANCE",
  "message": "服务器维护中，请稍后重试"
}
```

---

## 五、iOS 端接入清单

- [ ] 所有请求添加 `X-Device-Id` 和 `X-Device-Platform: ios` 请求头
- [ ] 全局拦截器处理 `403 CLIENT_BANNED` 和 `503 MAINTENANCE` 响应
- [ ] 启动时调用 `GET /api/status` 判断维护状态
- [ ] SSE 监听 `maintenance`、`update_available`、`telemetry_request` 事件
- [ ] 实现 `POST /api/telemetry` 上报逻辑（含被动响应 `telemetry_request`）
