# WordForge v1.0 客户端对接 — 接口变更清单

> 基线：v0.6.0-beta.4 → v1.0.0
> 起草日期：2026-05-22
> 覆盖范围：仅变更部分（Breaking + 新增 + 字段调整 + SSE 事件 + 头部约束）
> 完整 OpenAPI：[`docs/openapi.yaml`](./openapi.yaml) · 完整端点参考：[`docs/api-endpoints.md`](./api-endpoints.md)

## 目录

1. [⚠️ Breaking — 必须迁移](#1-️-breaking--必须迁移)
2. [🆕 新增端点](#2--新增端点)
3. [📝 字段变更（已有端点）](#3--字段变更已有端点)
4. [📡 SSE 事件表 — v1 新增 8 种](#4--sse-事件表v1-新增-8-种)
5. [🔒 strict-mode 头部强制](#5--strict-mode-头部强制)
6. [🔗 OpenAPI 规格](#6--openapi-规格)
7. [🛡️ Release 签名验证](#7-️-release-签名验证)
8. [迁移检查清单](#8-迁移检查清单)

---

## 1. ⚠️ Breaking — 必须迁移

### 1.1 `/api/v1/*` 全部废止（M0-C5）

老路径全部返回 `410 Gone`：

```json
{
  "error": "GONE",
  "message": "该端点已永久废止，请迁移至 /api/learning/* 或 /api/records/*",
  "sunset": "2027-05-22"
}
```

附响应头（RFC 8594）：

```http
Deprecation: true
Sunset: Sat, 22 May 2027 00:00:00 GMT
Link: </api/learning>; rel="successor-version"
```

**迁移对照**：`/api/v1/records` → `/api/records`，其余 v1 路径同理（详见 [`api-endpoints.md`](./api-endpoints.md) 第 18 节）。

### 1.2 `WordState` wire 序列化改 lowercase（M0-C1）

之前 `PascalCase` → 现在 `lowercase`：

| 旧 | 新 |
|---|---|
| `New` | `new` |
| `Learning` | `learning` |
| `Familiar` | `familiar` |
| `Mastered` | `mastered` |
| `Skipped` | `skipped` |

涉及所有 records / word-states / favorites 端点的 `state` 字段读写（含请求 body 与响应 body）。

---

## 2. 🆕 新增端点

### 2.1 `GET /api/users/me/export` — GDPR 数据导出（M1-G1）

```http
GET /api/users/me/export
Authorization: Bearer <token>
```

**响应**（`application/x-ndjson`，流式逐行）：

```jsonl
{"table":"profile","data":{...}}
{"table":"study_config","data":{...}}
{"table":"records","data":[...]}
{"table":"word_states","data":[...]}
{"table":"favorites","data":[...]}
{"table":"notes","data":[...]}
{"table":"sessions","data":[...]}
```

**频率限制**：每用户每 24h 1 次。超限返回 `429`：

```json
{ "success": false, "code": "GDPR_EXPORT_RATE_LIMITED", "message": "每 24 小时只能导出一次数据" }
```

附 `Retry-After: <秒数>` 响应头。

### 2.2 `POST /api/telemetry/error` — 前端 ErrorBoundary 上报（S6）

**无鉴权**（错误火焰即忘，不落库只 tracing log），body limit 64 KB：

```http
POST /api/telemetry/error
Content-Type: application/json

{
  "message": "ReferenceError: x is not defined",
  "stack": "at App.tsx:42:5\n  ...",
  "url": "https://wordforge.app/learning/session/abc",
  "userAgent": "Mozilla/5.0...",
  "componentStack": "in <SessionPage>\n  in <App>"
}
```

**响应**：`{"received": true}`，失败 500 不影响 UX。

### 2.3 `PATCH /api/admin/feedback/:id` — admin 反馈状态更新（M1-G3）

```http
PATCH /api/admin/feedback/{id}
Authorization: Bearer <admin-token>
Content-Type: application/json

{
  "priority": "high",              // low / normal / high / urgent
  "status": "in_progress",         // open / in_progress / resolved / closed
  "assigneeAdminId": "admin_xyz",  // 或 null 取消指派
  "resolution": "已重现，分配给后端组"
}
```

所有字段均为 `Optional`，只传需要更新的字段。响应返回更新后的完整 feedback item。`404` 表示 id 不存在。

### 2.4 `POST /api/admin/settings/maintenance` — 维护模式即时切换（S4）

```http
POST /api/admin/settings/maintenance
Authorization: Bearer <admin-token>
Content-Type: application/json

{ "active": true }
```

立即写库 + 触发 `SseEvent::Maintenance { active }` 广播给所有在线 admin 与用户 SSE 连接。

### 2.5 `GET /metrics` — Prometheus 端点（M0-P1，admin / 监控用）

文本格式（非 JSON）：

```
# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total{method="GET",path="/api/learning/session",status="200"} 1234
...
amas_process_event_duration_seconds_bucket{le="0.01"} ...
http_request_duration_seconds_bucket{...} ...
worker_last_run_timestamp_seconds{worker="error_rate_watchdog"} ...
```

客户端通常不直接消费，admin 监控面板与 Prometheus scraper 用。

---

## 3. 📝 字段变更（已有端点）

### 3.1 `GET /api/health` 加 `error_rate`（S7）

```json
{
  "status": "ok",
  "version": "1.0.0",
  "uptime_seconds": 86400,
  "error_rate": 0.0012   // ← 新增：滚动 5 分钟 5xx/total
}
```

### 3.2 `GET /api/feedback/items` 列表项新增 4 字段（M1-G3）

```json
{
  "id": "fb_xxx",
  "category": "bug",
  "content": "...",
  // —— 新增 4 字段 ——
  "priority": "normal",
  "status": "open",
  "assigneeAdminId": null,
  "resolvedAt": null,
  "resolution": null
}
```

### 3.3 `GET /api/admin/settings` 加 LLM 月度成本上限（M1-G2）

```json
{
  "max_users": 100,
  "registration_enabled": true,
  "maintenance_mode": false,
  "default_daily_words": 20,
  // —— 新增 ——
  "llm_advisor_max_cost_per_month_yuan": 100.0,
  "llm_advisor_current_month_spent_yuan": 23.45
}
```

---

## 4. 📡 SSE 事件表 — v1 新增 8 种

**统一格式**：`{"type":"<rename>", ...payload}`，事件名一律 `snake_case`，payload 字段一律 `camelCase`。

| `type` | 新增于 | payload |
|---|---|---|
| `new_llm_suggestion` | M0-C3 | `{ "suggestionId": i64 }` |
| `release_available` | M0-C3 | `{ "latestTag": "v1.0.0", "channel": "stable" \| "beta" }` |
| `update_progress` | M0-C3 | `{ "phase": "downloading", "percent": 45 }` |
| `probe_request` | M0-C3 | `{ "requestId", "batchId", "scriptB64", "timeoutMs", "ctxVersion" }` |
| `probe_confirm` | M0-C3 | `{ "requestId", "confirmToken" }` |
| `incident` | M0-P4 | `{ "errorRate": 0.025, "windowSecs": 300 }` |
| `worker_missed` | M1-A5 | `{ "workerName", "missCount": 3 }` |
| `llm_budget_exceeded` | M1-G2 | `{ "spentYuan", "capYuan", "resumeMonth": "2026-06" }` |

**注意**：`update_available` payload 重新设计含 `channel` 字段（v0.6.0-beta.3 加），客户端需按通道分类显示。

**完整事件清单**（14 个）：`maintenance` · `telemetry_request` · `banned` · `unbanned` · `data_corrupted` · `new_llm_suggestion` · `release_available` · `update_progress` · `update_available` · `probe_request` · `probe_confirm` · `incident` · `worker_missed` · `llm_budget_exceeded`。

权威定义：[`src/state.rs`](../src/state.rs) `pub enum SseEvent`。

---

## 5. 🔒 strict-mode 头部强制

`/api/telemetry` 与 `/api/learning/session-start` 在 strict-mode 开启时必须携带：

```http
X-Device-Id: <设备唯一 ID>
X-Device-Platform: ios | android | web | macos | windows | linux
```

payload 必须含：

```json
{
  "device": {
    "timezone": "Asia/Shanghai",
    "language": "zh-CN",
    // session_start 还要：
    "screenWidth": 390,
    "screenHeight": 844,
    "pixelRatio": 3.0,
    "cpuCores": 6
  }
}
```

缺失返回 `400` + 错误码：

| code | 含义 |
|---|---|
| `MISSING_DEVICE_ID` | 缺 `X-Device-Id` 头 |
| `MISSING_OS` | 缺 `X-Device-Platform` 头 |
| `MISSING_TIMEZONE` | payload 缺 `device.timezone` |
| `MISSING_LANGUAGE` | payload 缺 `device.language` |
| `MISSING_DEVICE_FINGERPRINT` | `session_start` 缺指纹四件套 |

**生产环境（8.135.57.148）**：strict-mode 已启用 `hard_block=true`，缺失立即 `400`。本地开发可关 `hard_block`，缺失只 `warn` log。

---

## 6. 🔗 OpenAPI 规格

| 资源 | 路径 |
|---|---|
| 静态规格（仓库） | [`docs/openapi.yaml`](./openapi.yaml) |
| 运行时端点 | `GET /api/openapi.json` |
| 详细端点参考 | [`docs/api-endpoints.md`](./api-endpoints.md) |
| SSE 事件枚举 | [`src/state.rs`](../src/state.rs) `SseEvent` |

**注意**：当前 `docs/openapi.yaml` 的 `info.version` 还是 `0.6.0-beta.4`，未跟 v1.0.0 bump。这是已知 v1.1 修复项，不影响契约本身。

OpenAPI 当前覆盖 v1 stable 档约 25 个端点（utoipa 自动生成 + CI drift 防漂），未覆盖的端点请直接读源码 + 本文档。

---

## 7. 🛡️ Release 签名验证

客户端如实现自动升级（`/api/admin/updates/apply` 流程），必须验证 release tarball minisign 签名。

| 资源 | 值 |
|---|---|
| 公钥文件 | [`docs/security/wordforge-release.pub`](./security/wordforge-release.pub) |
| 公钥指纹 | `RWQIHmTQvseWZo0Vc1npFBKZ/mMhi1S6eWT8hQ85Cmum5ftRgz87Yqll` |
| 验签库（Rust） | `minisign-verify` crate |
| 验签 CLI | `brew install minisign` |
| 算法 | Ed25519 |

binary 本身已编译期内嵌公钥（`MINISIGN_PUBKEY` env），自更新 worker 自动验签，无签名或公钥不匹配 → 拒绝安装并回滚（M0-R2/R3）。

### 手动验证示例

```bash
gh release download v1.0.0 --pattern '*x86_64*'
minisign -Vm wordforge-linux-x86_64.tar.gz \
  -P RWQIHmTQvseWZo0Vc1npFBKZ/mMhi1S6eWT8hQ85Cmum5ftRgz87Yqll
# 期望：Signature and comment signature verified
```

---

## 8. 迁移检查清单

客户端升级到 v1.0 兼容版前请逐项确认：

- [ ] **代码搜索** `/api/v1/` — 全部替换为新路径或下线
- [ ] **代码搜索** `"WordState"` 序列化处 — 全部改为 lowercase（含枚举映射 / 反序列化器）
- [ ] **GDPR 入口** — 设置页加"导出我的数据"按钮，处理 429 退避
- [ ] **ErrorBoundary** — 接入 `/api/telemetry/error`
- [ ] **admin feedback UI** — 加 priority / status / assignee / resolution 四列编辑
- [ ] **admin settings** — 加 maintenance toggle + LLM 月度成本可视化
- [ ] **SSE handler** — 注册 8 个新事件类型；老的 `update_available` 取 `channel` 字段
- [ ] **strict-mode 头** — `X-Device-Id` + `X-Device-Platform` 全请求挂载
- [ ] **strict-mode payload** — telemetry / session-start 加 device 指纹
- [ ] **health 监控** — 取 `error_rate` 字段（如客户端有运维面板）
- [ ] **自动升级** — minisign 验签链路联调（如客户端实现自更新）

---

## 9. 反馈

迁移过程中发现的契约问题（缺字段 / 类型不一致 / 文档错漏）请提 issue 到：
https://github.com/Heartcoolman/wordforge/issues

标记 `client-migration` label，团队 24h 内响应。
