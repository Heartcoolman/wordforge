# 客户端对接远程探针 —— 集成手册

> 配套设计稿：[`docs/superpowers/specs/2026-05-19-remote-probe-design.md`](../superpowers/specs/2026-05-19-remote-probe-design.md)
> 管理员手册：[`docs/admin/remote-probe.md`](../admin/remote-probe.md)
>
> 本文面向「客户端实现者」—— 既包括官方 SolidJS Web 端（见 `frontend/src/workers/probe/`），也包括日后任何第二实现（Tauri、Electron、移动端 WebView 等）。

---

## 1. 客户端职责（一句话）

订阅一条由用户级 JWT 鉴权的 SSE 流 → 收到 `probe_request` 事件 → 在**沙箱**里 eval admin 下发的 JS 脚本 → 把 `ctx`（受限快照）作为唯一入参喂给脚本 → 把脚本返回值 + 触发的受控写动作 → 经 `POST /api/probe/results` 上报。受控写动作不在首次跑里执行，必须等 admin 二次确认推回的 `probe_confirm` 事件。

数据流：

```
admin REPL ──POST /api/admin/probe──► 后端
                                       │  SseEvent::ProbeRequest
                                       ▼
                       客户端 SSE ◄────┘
                              │
                              │  ① 采集 ctx 快照（主线程）
                              │  ② 启沙箱跑 script
                              │  ③ 检测到 _actions ≠ ∅
                              ▼
                       POST /api/probe/results { status: "confirm_required", confirmToken }
                                       │  缓存 ticket（60s TTL）
                                       ▼
                       admin REPL ──POST /probe/:req/confirm──► 后端
                                       │  校验 device_id 后 5 位 + 推 SseEvent::ProbeConfirm
                                       ▼
                       客户端 SSE ◄────┘
                              │
                              │  ⑤ 用缓存的 snapshot 重跑 → 主线程执行 _actions（clearCache→signOut→reload）
                              ▼
                       POST /api/probe/results { status: "ok", resultJson }
```

---

## 2. 前置依赖

| 项 | 要求 |
|---|---|
| **用户级登录** | 拿到 `accessToken`，且通过常规续签机制保持有效 |
| **设备指纹** | 每个客户端有稳定的 `device_id`（UUID），需通过 `X-Device-Id` 请求头携带到所有 probe 相关请求 |
| **SSE 通道** | 持续连接 `GET /api/sse/stream?token=<accessToken>`（或后端约定的 query/header 鉴权方式），按需自动重连 |
| **沙箱能力** | 可以 eval 任意字符串 JS 且**默认无 DOM / fetch / cookie / IndexedDB**。Web 端使用 Dedicated `Worker({ type: 'module' })`；其他端使用各自的隔离机制（QuickJS、JSC、V8 isolate 等） |
| **HTTPS** | 生产环境必须 HTTPS；script 是基础 64 编码而非加密，传输层泄露等同代码泄露 |

> ⚠️ 远程探针默认 `PROBE_ENABLED=false`。客户端**不需要**前置判断是否启用 —— 关闭时后端不会推 `probe_request`，连不上业务通路是预期行为。

---

## 3. SSE 事件契约

SSE event-name 与 JSON 一一对应。客户端要新增解析两个事件：

### 3.1 `probe_request`

```json
{
  "requestId": "uuid-v4",
  "batchId": "uuid-v4",
  "scriptB64": "cmV0dXJuIHsgdWE6IGN0eC5uYXYudWEgfTs=",
  "timeoutMs": 3000,
  "ctxVersion": 1
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `requestId` | string | 单设备维度的唯一标识，整个生命周期不变 |
| `batchId` | string | 同一次 admin 派发的多设备共享同一个 batchId（admin REPL 按它聚合卡片） |
| `scriptB64` | string | base64(utf-8) 编码的脚本体；解码后是合法的 JS 函数体（不是表达式，不要包 IIFE） |
| `timeoutMs` | number | admin 指定的超时，后端已 clamp 到 `[100, max_timeout_ms]`（默认上限 10000） |
| `ctxVersion` | number | 后端要求的 ctx schema 版本。客户端实现的 `CLIENT_CTX_VERSION` 必须**全等**匹配；否则立刻回 `unsupported_ctx_version` 不要尝试执行 |

### 3.2 `probe_confirm`

```json
{
  "requestId": "uuid-v4",
  "confirmToken": "uuid-v4"
}
```

仅在客户端先回过 `confirm_required` 后才会收到。`confirmToken` 是客户端首次生成、回传给后端、后端原样推回的 nonce —— 客户端必须比对一致才重跑。

---

## 4. 上报 endpoint：`POST /api/probe/results`

```
POST /api/probe/results
Host: ...
Authorization: Bearer <accessToken>
X-Device-Id: <device_id>
Content-Type: application/json
Body limit: 320 KB

{
  "requestId": "uuid-v4",
  "status": "ok" | "error" | "timeout" | "confirm_required" | "unsupported_ctx_version",
  "resultJson": <任意 JSON 值，可选>,
  "stderr": "string",     // 可选；error / timeout 必填便于排查
  "durationMs": 1234,     // 必填，Worker 内部测量
  "truncated": false,     // 必填；resultJson 是否经截断包装
  "confirmToken": "uuid"  // status=confirm_required 时必填，其他必须省略
}
```

服务端校验链：
1. **`device_id` 归属校验** —— 表里的 `device_id` 必须与请求头 `X-Device-Id` 完全一致，否则 403
2. **终态幂等保护** —— 已是 `ok / error / timeout / expired / unsupported_ctx_version / offline` 的不接受再次上报，回 `400 PROBE_ALREADY_COMPLETED`
3. **confirm_required 必须带 confirmToken** —— 缺失回 `400 PROBE_CONFIRM_TOKEN_MISSING`

**截断协议**：当 `resultJson` 序列化超 256 KB 时，把它包装成

```json
{ "_truncated_raw": "<原 JSON 前 256KB 字符串>" }
```

`truncated` 字段同步置 `true`。这样 admin REPL 拿到的就是一个合法 JSON，且能直观看到溢出。

---

## 5. ctx 白名单 schema v1

⚠️ **客户端必须实现这个 schema 完整字段集**，每次 schema 调整两端同时 +1 `CLIENT_CTX_VERSION`。

| 字段 | 类型 | 实现要点 |
|---|---|---|
| `ctx.nav.ua` | string | `navigator.userAgent`；非浏览器端模拟 |
| `ctx.nav.language` | string | `navigator.language` |
| `ctx.nav.languages` | string[] | `navigator.languages` |
| `ctx.nav.platform` | string | `navigator.platform` |
| `ctx.nav.hardwareConcurrency` | number | `navigator.hardwareConcurrency` |
| `ctx.nav.deviceMemory` | number? | Chrome 独有，可选 |
| `ctx.nav.connection` | `{ effectiveType, downlink, rtt }`? | NetworkInformation API，可选 |
| `ctx.nav.online` | boolean | `navigator.onLine` |
| `ctx.perf.memoryMB()` | `{ used, total, limit } \| null` | `performance.memory` 单位换 MB；非 Chrome 内核返回 `null` |
| `ctx.perf.entries({type?, limit?})` | PerformanceEntry[] | `performance.getEntries()` 过滤 |
| `ctx.perf.resourceTimingSummary()` | `{ count, slowestMs, topUrls[] }` | resource 类型聚合 |
| `ctx.time` | `{ now, tz, performanceNow }` | `Date.now()` / `Intl.DateTimeFormat().resolvedOptions().timeZone` / `performance.now()` |
| `ctx.storage.keys(which)` | string[] | which = `'local' \| 'session'`；默认 local |
| `ctx.storage.size(which)` | `{ count, bytes }` | bytes 用 `key.length + value.length` 估算（每字符 2 字节按 UTF-16 也可） |
| `ctx.storage.get(key, which)` | string \| null | **⚠️ 强制返回 `''` 或脱敏值** —— 防 token 误读；只能让 admin 看键名/大小 |
| `ctx.idb.list()` | string[] | `indexedDB.databases()`；不支持平台返回 `[]` |
| `ctx.idb.count(db, store)` | number | 实现成本高时可一律返 `-1`（M2 当前实现） |
| `ctx.app.route` | string | 当前路由 / 页面标识 |
| `ctx.app.version` | string | 客户端版本号 |
| `ctx.app.buildHash` | string | 构建 hash（可选） |
| `ctx.app.storeSnapshot()` | object | **白名单字段**，禁止全量 dump 全局 store |
| `ctx.logs.tail(n=50)` | LogEntry[] | 见 §6 ring buffer |
| `ctx.errors.recent(n=50)` | ErrorEntry[] | 见 §6 |
| `ctx.net.recent(n=50)` | NetEntry[] | 见 §6 |
| `ctx.cmd.reload()` | void | 受控写：push `{type:'reload'}` 到 `_actions`，**不执行** |
| `ctx.cmd.clearCache()` | void | 受控写：push `{type:'clearCache'}` |
| `ctx.cmd.signOut()` | void | 受控写：push `{type:'signOut'}` |

**关键设计**：

- ctx **是值快照**，不是 live 代理。主线程一次性采集，序列化为 `CtxSnapshot`，传入 Worker；Worker 内 `buildCtx(snapshot, actions)` 把值包装为方法对象。这样 script 内同步取值，且无 MessagePort 复杂度
- `_actions` 是 Worker 内 cmd stub 的 push 队列。Worker 通过 `postMessage({ok, result, actions, durationMs})` 把它带回主线程
- `ctx.storage.get()` 的 value **必须脱敏**（强制空串）—— 这是隐私契约，任何端的实现都不可破

参考实现：`frontend/src/workers/probe/{ctx-factory,build-ctx,types}.ts`。

---

## 6. ring buffer 注入（启动时一次性）

`logs / errors / net` 这三个 ctx 字段需要客户端在**应用启动早期**注入全局 hook，把发生的日志/错误/网络请求写入定容环形 buffer。

| Buffer | 容量 | hook 点 |
|---|---|---|
| `logs` | 200 | `console.{log,warn,error,info,debug}` —— wrap 原方法，先 push 再透传 |
| `errors` | 50 | `window.addEventListener('error', ...)` + `'unhandledrejection'` |
| `net` | 100 | `window.fetch` 包装 + XHR 包装（如果用 axios，等价于 axios interceptor） |

幂等性：必须保证多次调用 `installRingBuffers()` 不重复 wrap。参考 `frontend/src/workers/probe/ring-buffers.ts`。

**敏感字段过滤**：

- `logs.message`：单条 ≤ 1024 char 截断
- `net`：**只记 `url, method, status, durationMs`**，**禁记 body / headers / cookies**

---

## 7. 沙箱执行规范

### 7.1 执行环境

| 平台 | 推荐沙箱 |
|---|---|
| Web | `new Worker(url, { type: 'module' })`，跑完立即 `terminate()` |
| Tauri/Electron | 渲染进程 + iframe sandbox 或独立 Worker |
| 移动 WebView | 同 Web |
| 纯 Node 端 | `vm.createContext({ ctx })` |

### 7.2 执行模式

```js
const fn = new Function('ctx', script);
const result = fn(ctx);
```

约定 admin 写的是**函数体**（包含 `return`），不是表达式。例如：

```js
return { ua: ctx.nav.ua, mem: ctx.perf.memoryMB() };
```

### 7.3 超时

主线程注册 `timeoutMs + HARD_KILL_GUARD_MS`（建议 +500ms 容差）后强制 `worker.terminate()`，回 `status: "timeout"`。

### 7.4 结果序列化

```ts
function serializeAndTruncate(value: unknown): { json: string; truncated: boolean } {
  let json = JSON.stringify(value);
  if (typeof json !== 'string') return { json: '[unserializable]', truncated: false };
  if (json.length > 256 * 1024) return { json: json.slice(0, 256 * 1024), truncated: true };
  return { json, truncated: false };
}
```

`truncated=true` 时上报：

```json
{ "resultJson": { "_truncated_raw": "<json prefix>" }, "truncated": true }
```

---

## 8. 受控写（D 类）执行流

### 8.1 首次执行（actions 非空）

1. Worker 跑完，回主线程 `{ ok, result, actions: [...], durationMs }`
2. 主线程**不执行 actions**，转而：
   - 本地缓存 `<requestId> → { script, snapshot, batchId, timeoutMs, confirmToken, expiresAt }`（TTL 60s）
   - `confirmToken` 客户端**自己生成**（UUID v4 或 `crypto.randomUUID()`）
   - 上报 `POST /api/probe/results`：

```json
{
  "requestId": "...",
  "status": "confirm_required",
  "resultJson": { "_actions": [...], "_preview": <脚本返回值> },
  "durationMs": 12,
  "truncated": false,
  "confirmToken": "uuid-客户端生成"
}
```

### 8.2 二次确认重跑

收到 `probe_confirm` 事件：

1. 取出缓存的 entry —— **缓存 miss 直接回 error**（admin 看到「TTL 已过」）
2. 比对 `payload.confirmToken === cached.confirmToken` —— 不一致回 error
3. 删缓存，用 `cached.snapshot` 重跑 script（Worker 内 cmd stub 仍会再 push 一遍 actions，但这次主线程会执行）
4. 上报正常 `status: "ok"` 结果（带 resultJson）
5. **接着**在主线程按固定序执行 actions

### 8.3 actions 执行序固化

```
clearCache → signOut → reload
```

**signOut 后必须中断**，reload 不再执行（页面已跳走）。原因：signOut 会把用户踢到登录页，再 reload 会闪一下登录页。

- `clearCache`：localStorage + sessionStorage + Cache Storage（**不动 IndexedDB**，避免误删本地学习数据）
- `signOut`：清 token + `window.location.assign('/admin/login')` 或等价
- `reload`：`window.location.reload()`

参考 `frontend/src/workers/probe/api-bridge.ts::executeActions`。

---

## 9. 错误码与状态机

### 9.1 status 字段值

| status | 何时上报 | 是否终态 |
|---|---|---|
| `ok` | 脚本正常返回 | 是 |
| `error` | new Function 异常 / Worker onerror / 主线程链路异常 | 是 |
| `timeout` | 超过 `timeoutMs + HARD_KILL_GUARD_MS` | 是 |
| `unsupported_ctx_version` | 收到 `probe_request` 时 `ctxVersion !== CLIENT_CTX_VERSION` | 是 |
| `confirm_required` | script 调了 `ctx.cmd.*` 且本次是首次执行 | 否（待 confirm） |

### 9.2 服务端 4xx/5xx

| HTTP 码 | code | 客户端处理 |
|---|---|---|
| 400 | `MISSING_DEVICE_ID` | bug，必带 `X-Device-Id` |
| 400 | `INVALID_PROBE_STATUS` | bug，bound status 枚举 |
| 400 | `PROBE_CONFIRM_TOKEN_MISSING` | confirm_required 漏带 token |
| 400 | `PROBE_ALREADY_COMPLETED` | 已是终态；不重试，丢弃日志即可 |
| 403 | `FORBIDDEN` | device_id 不匹配；可能 token 串了，丢弃 |
| 404 | `NOT_FOUND` | requestId 不存在；可能后端已 cleanup，丢弃 |
| 503 | `PROBE_DISABLED` | 不会出现（disabled 时根本收不到 SSE）|

⚠️ **失败重试策略**：上报失败**不重试**，写本地日志即可。重试可能导致 admin 看到重复卡片，得不偿失。

---

## 10. 安全闸 / 资源限制（实现者必须落地）

| 项 | 限额 | 落点 |
|---|---|---|
| script 解码后长度 | 16 KB（后端已校验，客户端可不重复校验） | — |
| `timeoutMs` 范围 | `[100, max_timeout_ms]`，后端已 clamp | — |
| `resultJson` 大小 | 256 KB（客户端截断） | 主线程 `serializeAndTruncate` |
| request body 上限 | 320 KB | 后端层 |
| Worker 实例 | 每个 request 起一个独立 Worker，结束 `terminate()` | — |
| 主线程 confirm 缓存 | 60s TTL | `pendingConfirms` Map |
| 沙箱 DOM 暴露 | **禁止** | Worker 天然隔离；其他端需明确移除 globalThis 引用 |
| `ctx.storage.get()` value | **强制返回 ''** | 隐私契约 |
| `net` buffer 记录 | 仅 `url/method/status/durationMs`，**不含 body/headers** | hook 实现 |

---

## 11. 启动顺序（建议）

伪代码：

```ts
// 1. 应用入口，越早越好（确保后续日志/错误/请求都进 buffer）
installRingBuffers();

// 2. 用户登录完成后（拿到 accessToken + device_id）
startProbeBridge(); // 内部 connectSseStream({ onProbeRequest, onProbeConfirm })

// 3. 用户登出
stopProbeBridge();  // 关 SSE，清 pendingConfirms
```

参考：`frontend/src/App.tsx` 的 `onMount` 钩子。

---

## 12. 验证清单

实现完成后，在本地用 admin REPL `/admin/probe` 跑一遍：

- [ ] `return { ua: ctx.nav.ua }` —— 卡片几秒内显示 UA
- [ ] `return ctx.perf.memoryMB()` —— 卡片显示堆内存
- [ ] `return ctx.logs.tail(5)` —— 看到最近 5 条 console（提前在控制台打几条）
- [ ] `return ctx.net.recent(10)` —— 看到最近 10 条 fetch 记录
- [ ] `return ctx.storage.get('access_token', 'local')` —— **返回空串而非真值**（隐私验证）
- [ ] `while(true){}` —— 3s 后卡片显示 timeout
- [ ] `return new Array(50000).fill({k:'x'.repeat(100)})` —— 卡片显示 `_truncated_raw`
- [ ] `ctx.cmd.reload(); return {ok:1}` —— 卡片「需确认」→ admin 输对 5 位 → 客户端实际刷新；输错 → 不动
- [ ] dispatch 时 admin 把 ctx_version 改成 99 —— 卡片显示 `unsupported_ctx_version`

---

## 13. 版本协商

ctx schema 一旦发布，**字段只增不减、不改名**。需要破坏性变更时：

1. 后端 `PROBE_CTX_VERSION_LATEST` +1
2. 各客户端 `CLIENT_CTX_VERSION` 同步 +1
3. 老客户端会自动回 `unsupported_ctx_version`，admin 在 REPL 收到提示后通知用户升级
4. dispatch 时后端把当前 `PROBE_CTX_VERSION_LATEST` 嵌入 `probe_request.ctxVersion`，无 fallback

不允许「ctx 字段静默缺失」—— 字段集是契约，缺一个就升版本。

---

## 14. 参考实现索引

| 模块 | 文件 |
|---|---|
| 类型契约 | `frontend/src/workers/probe/types.ts` |
| Ring buffer | `frontend/src/workers/probe/ring-buffers.ts` |
| ctx 采集（主线程） | `frontend/src/workers/probe/ctx-factory.ts` |
| ctx 包装（Worker 内） | `frontend/src/workers/probe/build-ctx.ts` |
| 沙箱 eval | `frontend/src/workers/probe/runner.worker.ts` |
| 主线程编排 / 确认链 / 序列化 | `frontend/src/workers/probe/api-bridge.ts` |
| SSE 解析分支 | `frontend/src/api/client.ts`（`onProbeRequest` / `onProbeConfirm`） |
| 启动钩子 | `frontend/src/App.tsx`（`installRingBuffers` + `startProbeBridge`） |
| 后端 SSE 事件 | `src/state.rs::SseEvent::{ProbeRequest, ProbeConfirm}` |
| 上报端点 | `src/routes/probe_results.rs` |

测试覆盖：`frontend/tests/workers/probe/{ring-buffers,build-ctx,ctx-snapshot,serialize-truncate,executeActions}.test.ts`。
