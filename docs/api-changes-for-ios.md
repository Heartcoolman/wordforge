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
- 上报间隔：**5 秒**（原 5 分钟）
- Body 大小限制：64 KB

**请求体：**

```json
{
  "eventType": "session_start",
  "requestId": null,
  "clientTs": "2026-04-11T10:00:00Z",
  "payload": {
    "device": {
      "cpuCores": 8,
      "memoryGb": 4.0,
      "screenWidth": 390,
      "screenHeight": 844,
      "pixelRatio": 3.0,
      "osName": "iOS 17.4",
      "browserName": "Safari",
      "browserVersion": "17.4",
      "timezone": "Asia/Shanghai",
      "language": "zh-CN",
      "touchSupport": true,
      "onlineStatus": true
    },
    "behavior": {
      "currentRoute": "/learn",
      "clickCount": 3,
      "clickTargets": [{ "label": "开始学习", "tag": "button" }],
      "scrollDepthPct": 42.5,
      "visibilityChanges": 0,
      "routeChanges": 1
    },
    "sessionDurationSecs": 120,
    "errorCount": 0,
    "actionsPerMin": 5.2,
    "avgResponseTimeMs": 230.5,
    "featureUsage": { "flashcard": 3 }
  }
}
```

**`eventType` 取值：**

| 值 | 触发时机 | `device` 字段 |
|---|---|---|
| `session_start` | SSE 连接建立后**立即**发送一次 | **必填**，包含完整设备指纹 |
| `periodic` | 此后每 5 秒发送 | 可省略 |
| `on_demand` | 收到 SSE `telemetry_request` 事件后发送 | 可省略 |

> `session_start` 的 `device` 对象用于服务端建立设备指纹档案，后续 `periodic` 事件中可省略。

**其他字段说明：**

| 字段 | 类型 | 说明 |
|---|---|---|
| `requestId` | string? | 仅 `on_demand` 时必填（来自 SSE `telemetry_request` 事件） |
| `clientTs` | string | 客户端时间戳（ISO 8601 UTC） |
| `payload.behavior` | object? | 自上次上报以来的行为增量，`session_start` 时可填零值 |
| `payload.sessionDurationSecs` / `errorCount` | integer | 不可为负数 |
| `payload.actionsPerMin` / `avgResponseTimeMs` | float | 不可为负数 |

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

### `data_corrupted` — 客户端心跳超时锁定 ⚠️

```json
{ "type": "data_corrupted" }
```

**触发条件**：服务端连续 5 次（约 25 秒）未收到该设备的遥测心跳。

**客户端必须实现：**
1. 展示**全屏不可关闭**弹窗，标题「数据损坏」，正文「客户端数据已损坏，请重启应用后再试。」
2. 禁止所有用户交互（不可导航、不可点击、不可关闭弹窗）
3. SSE 连接与遥测上报继续运行（UI 仍锁定）
4. 用户重启应用/刷新后锁定自然解除；若遥测恢复正常，下一个 25 秒周期内服务端不会重复推送

> 注意：`data_corrupted` 事件本身无附加字段。若设备持续无心跳，服务端每 25 秒重复发送一次。

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

**基础接入（原有）：**
- [ ] 所有请求添加 `X-Device-Id` 和 `X-Device-Platform: ios` 请求头
- [ ] 全局拦截器处理 `403 CLIENT_BANNED` 和 `503 MAINTENANCE` 响应
- [ ] 启动时调用 `GET /api/status` 判断维护状态
- [ ] SSE 监听 `maintenance`、`update_available`、`telemetry_request` 事件

**遥测增强（新增）：**
- [ ] SSE 建立连接后立即发送一次 `session_start` 事件，`payload.device` 携带完整设备指纹
- [ ] 此后每 **5 秒**发送一次 `periodic` 事件（含 `behavior` 增量）
- [ ] 收到 `telemetry_request` 时发送 `on_demand` 事件
- [ ] SSE 监听 `data_corrupted` 事件，触发后展示全屏锁定提示，禁止所有交互
