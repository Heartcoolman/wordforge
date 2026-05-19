# 远程探针（Remote Probe）设计文档

**日期**：2026-05-19
**作者**：brainstorming 会话产出
**状态**：待用户复核
**关联模块**：admin 控制台、telemetry worker、SSE 通道

---

## 1. 背景与目标

### 1.1 现状

* 现有遥测下发：`POST /api/admin/clients/:id/request-telemetry` → SSE 推 `SseEvent::TelemetryRequest { request_id }`。
* 客户端 `frontend/src/workers/telemetry.ts` 收到后调用 `sendTelemetry('on_demand', requestId, includeDevice=true)`。
* 采集内容**编译期写死**：device fingerprint（UA / 屏幕 / 时区 / CPU 核数 / 内存 GB / 触控 / 在线状态）+ behavior buffer（点击数、滚动深度、route 切换数）。
* 上报：`POST /api/telemetry` → 入 `telemetry_events`（原始 JSON）+ `telemetry_summaries`（强类型摘要字段）。

### 1.2 痛点

admin 「触发遥测」的实际语义只是「让客户端立刻上报预制信息」。admin 无法在采集前指定「这次我想要什么」。新增字段必须改前端 + 后端 + 表 + 类型，迭代成本高。

### 1.3 目标

* admin 在控制台编写表达式 / 选模板，下发到指定（或一批 / 全部在线）客户端，客户端在受限沙箱里执行并把结果近实时返回，REPL 式体验。
* 客户端「能采集什么」由 ctx 白名单决定，扩展能力走代码版本而非协议改动。
* 全量留痕，可审计。受控写动作需二次确认。

---

## 2. 整体架构

### 2.1 数据流

```
┌──────────────────┐        ┌────────────────────┐
│ Admin REPL 面板   │        │ Web 客户端          │
│ /admin/probe     │        │ (telemetry worker) │
└────────┬─────────┘        └──────────▲─────────┘
         │ ① POST                      │ ② SSE 推 ProbeRequest
         │   /api/admin/probe          │   { request_id, script_b64,
         ▼                             │     timeout_ms, ctx_version }
┌─────────────────────────────────────┴────────────┐
│            后端 (axum)                            │
│  ├── POST /api/admin/probe        下发 + 审计落库 │
│  ├── GET  /api/admin/probe/:bid/stream  SSE 拉结果│
│  ├── POST /api/admin/probe/:rid/confirm 二次确认  │
│  ├── GET  /api/admin/probe / :rid       历史查询  │
│  ├── POST /api/probe/results            客户端回传│
│  └── SQLite: probe_executions     不可变审计 + result│
└──────────────────────────────────────────────────┘
                                       ▲
                                       │ ③ Dedicated Worker
                                       │   new Function('ctx', body)(ctx)
                                       │   3s timeout / 256KB 截断
                                       │ ④ POST /api/probe/results
                                       │   { request_id, status,
                                       │     result_json, stderr,
                                       │     duration_ms, truncated }
```

### 2.2 与现有遥测的关系

| 现有 | 新增 |
|---|---|
| `SseEvent::TelemetryRequest` | `SseEvent::ProbeRequest` / `ProbeConfirm` |
| `POST /api/admin/clients/:id/request-telemetry` | `POST /api/admin/probe` |
| `POST /api/telemetry` | `POST /api/probe/results` |
| `telemetry_events` / `telemetry_summaries` | `probe_executions` |
| `frontend/src/workers/telemetry.ts`（不动） | `frontend/src/workers/probe/*`（新增） |

两条链路**完全独立**，仅共享 SSE 连接通道与 admin 鉴权中间件。

### 2.3 范围声明（YAGNI）

* ✅ 单次下发 + 实时回传
* ✅ broadcast（一次下发多设备 / 全部在线）
* ✅ D 类受控写（reload / clearCache / signOut）+ 二次确认
* ❌ 订阅式（一次下发周期上报）
* ❌ 用户侧"我同意被 probe"开关（内部工具，admin 信任模型已覆盖）
* ❌ 富文本 / 二进制 result
* ❌ probe 历史的高级搜索 / 全文检索（按 batch / device / admin 列表足够）

---

## 3. 协议契约

### 3.1 SSE 事件

`src/state.rs` 扩展：

```rust
pub enum SseEvent {
    TelemetryRequest { request_id: String },  // 现有，不动
    ProbeRequest {
        request_id: String,
        batch_id: String,
        script_b64: String,    // base64，避免 SSE data 行换行问题
        timeout_ms: u32,       // 默认 3000，上限 10000
        ctx_version: u32,      // 客户端校验 schema 匹配
    },
    ProbeConfirm {
        request_id: String,
        confirm_token: String,
    },
}
```

### 3.2 REST 端点

#### 3.2.1 下发

```
POST /api/admin/probe
Auth: admin
Body:
{
  "targets": {
    "device_ids": ["d1", "d2"],   // 二选一
    "all_online": false
  },
  "script": "return { ua: ctx.nav.ua, memMB: ctx.perf.memoryMB() };",
  "timeout_ms": 3000,
  "note": "调查 Edge 用户反馈的 OOM"
}
Response 200:
{
  "batch_id": "b-uuid",
  "dispatched": [
    { "device_id": "d1", "request_id": "r-uuid1" },
    { "device_id": "d2", "request_id": "r-uuid2" }
  ],
  "skipped_offline": []
}
Response 429: 限速命中
Response 400: script 超长 / timeout_ms 越界 / targets 形状非法
```

#### 3.2.2 拉结果（REPL 用）

```
GET /api/admin/probe/:batch_id/stream
Auth: admin
返回 SSE 流：
  event: result
  data: { device_id, request_id, status, result_json, duration_ms, stderr, truncated }
  event: completed
  data: { received: N, expected: N }
```

#### 3.2.3 二次确认

```
POST /api/admin/probe/:request_id/confirm
Auth: admin
Body: { "confirm_token": "...", "device_id_suffix": "abc12" }
Response 200: { "confirmed": true }
Response 400: device_id_suffix 不匹配 / confirm_token 失效
```

#### 3.2.4 历史查询

```
GET /api/admin/probe?batch_id=&device_id=&admin_id=&limit=50&offset=0
GET /api/admin/probe/:request_id
Response: probe_executions 行（含 script_body / result_json）
```

#### 3.2.5 客户端回传

```
POST /api/probe/results
Auth: user token + deviceId header
Body:
{
  "request_id": "r-uuid",
  "status": "ok" | "error" | "timeout" | "confirm_required" | "unsupported_ctx_version",
  "result_json": { ... },           // status=ok 时
  "stderr": "ReferenceError: ...",  // 非 ok 时（可选）
  "duration_ms": 42,
  "truncated": false,
  "confirm_token": "..."            // 仅 status=confirm_required 时
}
Response 200: { ok: true }
Response 4xx: 校验失败
```

#### 3.2.6 API status 与表 status 映射

| 客户端回传 status（API） | 写入表 status |
|---|---|
| `ok` | `ok` |
| `error` | `error` |
| `timeout` | `timeout` |
| `confirm_required` | `confirm_pending`（等待 admin 确认） |
| `unsupported_ctx_version` | `unsupported_ctx_version` |

后端自有的终态：`offline`（下发时即落）、`expired`（confirm_token TTL 60s 过期未确认）。

### 3.3 D 类受控写时序

```
admin POST /api/admin/probe (script 含 ctx.cmd.reload())
  └─> 后端写 probe_executions(status=pending) → SSE 推 ProbeRequest
       └─> 客户端 sandbox detect ctx.cmd. → 不执行 cmd → POST results
            { status:"confirm_required", confirm_token:"..." }
            └─> 后端 update probe_executions(status=confirm_pending)
                 └─> admin 控制台 SSE 收 result → 弹"输入 device_id 后5位"
                      └─> admin POST /confirm { token, suffix }
                           └─> 后端校验 → SSE 推 ProbeConfirm
                                └─> 客户端用同一 ctx 快照重跑（cmd stub 解锁）
                                     └─> POST results { status:"ok", ... }
                                          └─> 主线程执行 _actions（reload 等）
```

---

## 4. 表结构

```sql
CREATE TABLE IF NOT EXISTS probe_executions (
    id TEXT PRIMARY KEY,              -- = request_id
    batch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    admin_id TEXT NOT NULL,
    admin_username TEXT NOT NULL,
    script_body TEXT NOT NULL,        -- 全量留痕
    script_sha256 TEXT NOT NULL,
    has_cmd_call INTEGER NOT NULL DEFAULT 0,
    note TEXT,
    timeout_ms INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'pending', 'confirm_pending', 'ok', 'error',
        'timeout', 'offline', 'expired', 'unsupported_ctx_version'
    )),
    result_json TEXT,
    stderr TEXT,
    duration_ms INTEGER,
    truncated INTEGER NOT NULL DEFAULT 0,
    dispatched_at TEXT NOT NULL,
    confirmed_at TEXT,
    completed_at TEXT
);
CREATE INDEX idx_probe_exec_batch  ON probe_executions(batch_id, dispatched_at DESC);
CREATE INDEX idx_probe_exec_device ON probe_executions(device_id, dispatched_at DESC);
CREATE INDEX idx_probe_exec_admin  ON probe_executions(admin_id, dispatched_at DESC);
CREATE INDEX idx_probe_exec_pending ON probe_executions(status, dispatched_at)
    WHERE status IN ('pending', 'confirm_pending');
```

**不提供 DELETE 接口**。清理由独立 cron 软删（保留 ≥60 天，具体由部署侧配置）。

---

## 5. 客户端 Worker 沙箱

### 5.1 文件结构

```
frontend/src/workers/probe/
├── runner.worker.ts     # 在 Dedicated Worker 里跑，无 DOM/fetch/storage
├── api-bridge.ts        # 主线程：监 SSE → 采 ctx → 起 worker → 收结果 → POST
├── ctx-factory.ts       # 主线程构造 ctx 快照
├── ring-buffers.ts      # 全局环形 buffer（logs/errors/net），main bundle 初始化时注册
└── types.ts             # Ctx schema 与协议类型
```

### 5.2 ctx 是「快照」不是「live」

Worker 无 DOM / 无 IndexedDB / 无 storage / 无 fetch。把它们桥进去需要 MessagePort 异步往返，复杂且引入新风险面。**改为主线程一次性采集 → 序列化 → postMessage 进 Worker → 当 ctx 用**。

代价：script 不能在执行中再查 IDB / storage。
收益：sandbox 边界明确（Worker 内能拿到的就只有传进去的 JSON 值 + cmd stub）。

### 5.3 D 类 cmd 实现

Worker 里 `ctx.cmd` 是 stub，仅往内部 `_actions` 数组推记录：

```ts
const _actions: Array<{type: string}> = [];
ctx.cmd = {
  reload: () => { _actions.push({ type: 'reload' }); },
  clearCache: () => { _actions.push({ type: 'clearCache' }); },
  signOut: () => { _actions.push({ type: 'signOut' }); },
};
// 用户的 script 跑完，runner 把 _actions 附在结果上
return { ...userReturn, _actions };
```

主线程拿到结果后：
* `_actions.length > 0` 且当前为首次执行 → 阻断 → POST `status:"confirm_required"`
* `_actions.length > 0` 且已 confirmed → 顺序执行 actions（reload 等放在最后）

### 5.4 执行流

1. **main bundle 启动**：`ring-buffers.ts` 注册 console / `window.onerror` / `unhandledrejection` / fetch wrap 拦截器，写入环形 buffer（logs 200 / errors 50 / net 100）。
2. **收到 `ProbeRequest`**：`api-bridge.ts.handleProbe()`：
   1. `if (ctx_version !== CLIENT_CTX_VERSION)` → POST `unsupported_ctx_version`，return。
   2. `collectCtx()` 同步采集 nav / perf / time / storage 摘要 / app.route / app.version，**异步**采集 idb.list + idb.count（最多 200ms），从 ring buffer 取 logs/errors/net snapshot。
   3. `script_body = atob(script_b64)`；**粗略检测** `script_body.includes('ctx.cmd.')`，若是且未携带 confirm_token → 仍正常 spawn worker 执行（cmd 是 stub，会被推到 `_actions`），主线程收 result 后判断 `_actions` 阻断。
   4. spawn dedicated Worker，postMessage `{ script: script_body, ctx_snapshot }`。
   5. `setTimeout(timeout_ms)`：超时则 `worker.terminate()` + POST `timeout`。
3. **Worker 内**：`new Function('ctx', script_body)(ctx)`，try-catch，结果 + `_actions` postMessage 回主线程。
4. **主线程收 result**：
   * 若 `_actions` 非空且未 confirmed → POST `confirm_required`，缓存 ctx_snapshot 与 script 供 confirm 后重跑（带 `request_id` 索引，TTL 60s）。
   * 否则 `JSON.stringify(result)` 量 size → 超 256KB → 取前 262144 字节 + `truncated=true` → POST `ok`。
   * 若已 confirmed 且有 `_actions` → 按序执行 reload / clearCache / signOut（reload 始终最后；signOut 后不再 reload）。
5. **收到 `ProbeConfirm`**：从缓存（key=request_id，TTL 60s）取 ctx_snapshot 与 script → 重新 spawn worker → script 再次跑、`_actions` 再次填充 → 主线程在「已 confirmed」分支顺序执行 actions（reload 始终最后；signOut 后不再 reload）→ POST 最终 `status:"ok"` 结果。缓存未命中（TTL 过期 / 浏览器刷新）则回 `status:"error", stderr:"confirm cache miss"`，后端把表状态推进到 `expired`。

### 5.5 ring-buffer 实现

* `logs`：拦截 `console.log/warn/error/info/debug`，存 `{ level, ts, args: args.map(safeStringify) }`，env-gated（开发可关）。
* `errors`：监听 `window.error` 与 `unhandledrejection`，存 `{ ts, message, stack, source }`。
* `net`：wrap `window.fetch`，存 `{ ts, url, method, status, durationMs }`。**不存 body / headers**。

容量与策略：固定大小，循环覆盖；探针 tail 时直接读快照。

---

## 6. ctx 白名单 schema（v1）

```ts
export const CLIENT_CTX_VERSION = 1;

export type Ctx = {
  // A. 环境
  nav: {
    ua: string;
    language: string;
    languages: string[];
    platform: string;
    hardwareConcurrency: number;
    deviceMemory?: number;
    connection?: { effectiveType: string; downlink: number; rtt: number };
    online: boolean;
  };
  perf: {
    memoryMB: () => { used: number; total: number; limit: number } | null;
    entries: (filter?: { type?: string; limit?: number }) => PerformanceEntry[];
    resourceTimingSummary: () => {
      count: number;
      slowestMs: number;
      topUrls: Array<{ url: string; durationMs: number }>;
    };
  };
  time: { now: number; tz: string; performanceNow: number };

  // B. 应用状态
  storage: {
    keys: (which?: 'local' | 'session') => string[];
    get: (key: string, which?: 'local' | 'session') => string | null;
    size: (which?: 'local' | 'session') => { count: number; bytes: number };
  };
  idb: {
    // 主线程预先采集到快照，worker 里是同步访问值
    list: () => string[];                        // db names
    count: (db: string, store: string) => number;
  };
  app: {
    route: string;
    version: string;       // GIT_VERSION
    buildHash: string;
    storeSnapshot: () => unknown;  // pinia 去敏摘要（排除 token / 密码 / email 等字段）
  };

  // C. 诊断
  logs:   { tail: (n?: number) => LogEntry[] };
  errors: { recent: (n?: number) => ErrorEntry[] };
  net:    { recent: (n?: number) => NetEntry[] };  // 不含 body / headers

  // D. 受控写（push action stub，主线程根据 _actions 执行）
  cmd: {
    reload:     () => void;
    clearCache: () => void;
    signOut:    () => void;
  };
};
```

**ctx 扩展规则**：新增字段或方法 → `CLIENT_CTX_VERSION` +1。后端下发时 `ctx_version` 由 admin 控制台拉前端常量传入（或由后端管理后台的 schema 注入一致）。前端不匹配时回 `unsupported_ctx_version`，admin 自助升级前端版本后再用新 API。

**storeSnapshot 脱敏白名单**：在 ctx-factory.ts 维护 `STORE_SNAPSHOT_FIELDS` 数组，列出可暴露的 pinia 字段路径（如 `app.preferences`, `learning.currentSessionId`），其它一律不出。绝不包含 token / email / 用户名 / 真实姓名等。

---

## 7. 安全闸

| 闸 | 位置 | 默认 | 可配置 |
|---|---|---|---|
| 全量 payload + result 审计 | `probe_executions`，无 DELETE 接口 | 强制 | 否 |
| Per-admin 限速 | axum tower `RateLimit` layer on `POST /api/admin/probe`，key=admin_id | 10/min | 是（config.toml） |
| D 类二次确认 | 客户端 sandbox 检测 `_actions` + 后端 confirm 端点 | 强制 | 否 |
| 执行超时 | Worker `terminate()` | 3000ms | 是（请求级，上限 10000） |
| Result 大小 | 客户端 `JSON.stringify(result).length > 262144` → 截断 + truncated=true | 256KB | 否 |
| 离线设备处理 | 下发时检查 active_sse → `status='offline'`，不重试 | 自动 | 否 |
| ctx schema 兼容 | 客户端 `ctx_version` 校验 | 自动 | 否 |
| 全局 kill-switch | `config.toml [probe] enabled=false` 默认关 | 关 | 是 |
| 前端路由 gate | `enabled=false` 时 `/admin/probe` 404 | 自动 | 自动 |

**enabled=false 时的具体行为**：
* `POST /api/admin/probe` → 503 `PROBE_DISABLED`
* `GET /api/admin/probe*` → 503
* `POST /api/probe/results` → 仍接（防客户端发出后服务端关，不丢数据），但只写 `probe_executions` 不触发 cmd

---

## 8. 前端 Admin REPL 面板

### 8.1 路由与入口

* 新增路由 `/admin/probe`（在 admin 路由组下）。
* 管理后台侧栏新增「远程探针」入口。
* 不启用（`enabled=false`）时路由不挂载。

### 8.2 布局

三栏（上 / 中 / 下）：

```
┌────────────────────────────────────────────────────────────┐
│ § Target                                                   │
│ ○ 单设备  device_id: [______________]                       │
│ ○ 多设备  device_ids: [d1] [d2] [+]                         │
│ ○ 全部在线（当前 N 台）                                      │
├────────────────────────────────────────────────────────────┤
│ § Script (CodeMirror, JS mode)                             │
│ 模板: [▼ 设备指纹] [慢请求 Top5] [最近错误] [LS 摘要] [IDB] │
│ ┌──────────────────────────────────────────────────────┐  │
│ │ return {                                              │  │
│ │   mem: ctx.perf.memoryMB(),                           │  │
│ │   slow: ctx.net.recent(20)                            │  │
│ │              .filter(n => n.durationMs > 1000)        │  │
│ │ };                                                    │  │
│ └──────────────────────────────────────────────────────┘  │
│ timeout: [3000]ms  note: [_______________]   [发送]        │
├────────────────────────────────────────────────────────────┤
│ § 实时结果（按 device 分卡片）                              │
│ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐          │
│ │ d1 ✓ 42ms   │ │ d2 ⏱ 超时    │ │ d3 ✗ Refer.E│          │
│ │ {ua:"...",  │ │             │ │ stderr:...  │          │
│ │  mem:...}   │ │             │ │              │          │
│ └─────────────┘ └─────────────┘ └─────────────┘          │
│ [复制 JSON] [导出全部 batch] [回放此 script]               │
└────────────────────────────────────────────────────────────┘
```

模板定义：`frontend/src/views/admin/probe-templates.ts` 导出数组 `[{name, body, ctx_version_min}]`。

### 8.3 受控写确认对话框

* 当收到 `status:"confirm_required"` 的 result → 该设备卡片状态变为「需确认」，按钮：[确认执行] [取消]
* 点[确认执行] → modal：「即将对设备 `d1xxx2` 执行 `[reload]`，请输入该设备 ID 后 5 位：[_____]」
* 输入正确后 → `POST /api/admin/probe/:request_id/confirm`
* 取消 → POST 一个 abandon（或不发，让 confirm_token TTL 过期）

---

## 9. 测试策略

### 9.1 后端 (`cargo test`)

* `probe_dispatch_to_online_device`：device online → 落库 + 推 SSE
* `probe_dispatch_to_offline_device`：device offline → `status='offline'` 落库
* `probe_broadcast_all_online`：多设备 fan-out
* `probe_rate_limit_per_admin`：第 11 次请求 → 429
* `probe_confirm_happy_path`：confirm_required → confirm → ok
* `probe_confirm_wrong_suffix`：confirm 端点拒绝
* `probe_audit_immutable`：手动 DELETE 不能从公共 API 触发
* `probe_unsupported_ctx_version`：客户端回 `unsupported_ctx_version` → 状态正确落库
* `probe_disabled_503`：`enabled=false` 时所有 admin probe 端点 503

### 9.2 客户端 (`vitest`)

* `ctx-snapshot.test.ts`：collectCtx 输出符合 Ctx 类型，脱敏字段不出现
* `runner-timeout.test.ts`：模拟 while(true) → 3s 内 terminate → POST timeout
* `runner-truncate.test.ts`：return 大对象 → JSON > 256KB → truncated=true
* `cmd-confirm-flow.test.ts`：含 `ctx.cmd.reload()` script → 首次回 confirm_required，收 ProbeConfirm 后重跑并执行 reload
* `ctx-version-mismatch.test.ts`：ctx_version != CLIENT_CTX_VERSION → POST unsupported_ctx_version
* `ring-buffer-cap.test.ts`：写满 N+5 条 → 仅保留最近 N 条

### 9.3 集成 (`playwright`)

* admin 打开 `/admin/probe` → 自己当 target → `return ctx.nav.ua` → 卡片显示 UA
* 模板下拉「设备指纹」→ 一键填入 → 发送 → 收到完整指纹

---

## 10. 回滚与上线

### 10.1 上线步骤

1. release 包含新表迁移 + `[probe] enabled=false` 默认配置
2. 部署到生产，验证遥测正常无回归
3. admin 在 config.toml（或 admin/amas config UI）打开 `enabled=true`
4. 用 playwright 用例的方式自测一次
5. 通知质量部门可以开始用

### 10.2 回滚

* `enabled=false`：所有 admin probe 端点 503；客户端无 ProbeRequest 进入。
* 已写入的 `probe_executions` 数据保留不删。
* 新表对未启用环境零影响（SQLite 不读不写就不占内存）。

---

## 11. 未来扩展（不在 MVP）

* 订阅式探针：admin 设置「每 30 秒跑一次，跑 5 分钟」
* 探针 script library：公共模板沉淀，admin 之间分享
* 受限请求探针：允许 `ctx.fetch` 但仅限白名单 URL 模式（如 `/api/health`）
* 移动客户端支持（如未来有 macOS / iOS 原生客户端，ctx 需要重新定义）

---

## 附录 A：决策记录

| 决策 | 选项 | 选定 | 理由 |
|---|---|---|---|
| 自由度 | 白名单字段 / DSL / 远程 JS | 远程 JS | 用户明确要求最大灵活度 |
| 执行环境 | Worker / 主线程 eval / 裸 eval | Worker + ctx 白名单 | 安全边界最清晰，无 DOM 暴露 |
| ctx 范围 | A/B/C/D 四组 | 全选 | 覆盖环境、状态、诊断、控制四类需求 |
| 交互模型 | REPL / 异步 / 订阅 | REPL | 调试体验接近 DevTools |
| 结果通道 | 独立端点 / 复用 telemetry / SSE 反向 | 独立端点 + 独立表 | 与现有遥测语义隔离，审计独立 |
| 超时 | 3s / 10s / 30s | 3s | 多数 ctx 查询可在 3s 内完成 |
| 单次响应 | 64K / 256K / 1M | 256K | 装得下 logs/net/storage 摘要又不爆表 |
| 安全闸 | 审计 / 二次确认 / 限速 / 广播 | 全选 | 风险面要求 |
