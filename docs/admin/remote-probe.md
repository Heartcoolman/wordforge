# 远程探针（Remote Probe）操作手册

> 设计文档：[`docs/superpowers/specs/2026-05-19-remote-probe-design.md`](../superpowers/specs/2026-05-19-remote-probe-design.md)

admin 在控制台写 JS 表达式 → 通过 SSE 下发到指定 / 一批 / 全部在线客户端 → 客户端 Web Worker 沙箱里执行（仅可见白名单 `ctx`）→ 结果近实时回 admin。

## 1. 启用

默认 **关闭**。开启方式（任选其一）：

```bash
# 环境变量（重启生效）
export PROBE_ENABLED=true
```

可调参数（环境变量）：

| 变量 | 默认 | 含义 |
|---|---|---|
| `PROBE_ENABLED` | `false` | kill switch；false 时所有 admin probe 端点 503 |
| `PROBE_RATE_LIMIT_PER_MIN` | `10` | per-admin 每分钟最大 dispatch 次数 |
| `PROBE_MAX_TIMEOUT_MS` | `10000` | timeoutMs 上限 |
| `PROBE_DEFAULT_TIMEOUT_MS` | `3000` | 未指定 timeoutMs 时的默认值 |
| `PROBE_RETENTION_DAYS` | `60` | `probe_executions` 表行保留天数（cron 清理） |

未启用时：admin sidebar 入口仍可见但点击发送会得到「远程探针未启用」提示；DB 表存在但无数据写入。

## 2. 使用

### 2.1 单次下发

1. 打开 `/admin/probe`
2. **Target**：单设备（填 device_id）/ 多设备（空格 / 逗号分隔）/ 全部在线
3. **script**：textarea 写 JS 函数体，可用 `ctx` 参数。例如：
   ```js
   return {
     ua: ctx.nav.ua,
     mem: ctx.perf.memoryMB(),
   };
   ```
   或点 「📋 模板」一键填入预设。
4. **timeout (ms)**：100 ≤ x ≤ 10000，默认 3000
5. **note**：可选，落 DB 用于审计
6. 点 「发送」→ 几秒内卡片实时显示结果

### 2.2 ctx 白名单（v1）

| 字段 | 类型 | 说明 |
|---|---|---|
| `ctx.nav` | `{ ua, language, languages[], platform, hardwareConcurrency, deviceMemory?, connection?, online }` | navigator 三件套 + 网络 |
| `ctx.perf.memoryMB()` | `{ used, total, limit } \| null` | JS heap 占用（Chrome only） |
| `ctx.perf.entries({ type?, limit? })` | PerformanceEntry[] | 同 `performance.getEntries()` |
| `ctx.perf.resourceTimingSummary()` | `{ count, slowestMs, topUrls[] }` | 资源加载摘要 |
| `ctx.time` | `{ now, tz, performanceNow }` | 时间戳 + 时区 |
| `ctx.storage.keys('local'\|'session')` | string[] | LocalStorage / SessionStorage 键名 |
| `ctx.storage.size(which)` | `{ count, bytes }` | 大小 |
| `ctx.storage.get(key, which)` | `string \| null` | **value 已脱敏强制返回 ''**（防误暴露 token） |
| `ctx.idb.list()` | string[] | IndexedDB 库名 |
| `ctx.idb.count(db, store)` | number | objectStore 记录数（M2 中暂返 -1） |
| `ctx.app` | `{ route, version, buildHash, storeSnapshot() }` | 应用元信息 |
| `ctx.logs.tail(n=50)` | LogEntry[] | console 日志环形 buffer，最近 N 条 |
| `ctx.errors.recent(n=50)` | ErrorEntry[] | window.error + unhandledrejection |
| `ctx.net.recent(n=50)` | NetEntry[] | fetch 拦截记录（仅 url/method/status/durationMs） |
| `ctx.cmd.reload()` | void | 受控写：触发客户端 reload（**需二次确认**） |
| `ctx.cmd.clearCache()` | void | 受控写：清 LS+SS+Cache Storage（**不动 IDB**） |
| `ctx.cmd.signOut()` | void | 受控写：清 token + 跳 /admin/login |

### 2.3 D 类受控写 + 二次确认

当 script 调用 `ctx.cmd.*`：

```
admin 发送 → 客户端 Worker 跑 → 检测到 _actions 非空
  ↓
客户端回 status="confirm_required"（不执行 cmd），缓存 ctx 快照 60s
  ↓
admin 卡片显示「需确认」+「确认执行」按钮
  ↓
点按钮 → modal 弹「输入该设备 ID 后 5 位」
  ↓
admin 输入正确后 → POST /api/admin/probe/:req_id/confirm
  ↓
后端推 ProbeConfirm SSE → 客户端用同一 ctx 快照重跑
  ↓
主线程顺序执行 actions：clearCache → signOut → reload
  （signOut 后跳转，reload 短路）
```

**超时**：60s 内未确认 → 服务端 sweeper 把该 request 状态推进到 `expired`，admin REPL 卡片更新为「已过期」。

## 3. 审计与隐私

* **全量留痕**：每次 dispatch 在 `probe_executions` 落一行（含 `script_body / script_sha256 / admin_id / device_id / has_cmd_call / dispatched_at / status / result_json / stderr` 等）。
* **无 DELETE API**：公共接口不暴露删除。仅 60 天 cron 软删。
* **storage 值脱敏**：`ctx.storage.get()` 强制返回空字符串，admin 只能看键名 + 大小，看不到值（防 token 误读）。
* **限速**：per-admin 10/min；超 → 429（in-memory，重启清零）。
* **kill switch**：`PROBE_ENABLED=false` 即时关闭，已有数据保留。

## 4. 历史与回放

* 右侧 sticky 边栏「最近 batch」自动加载最近 10 个 batch 的首条
* 点 「回放此 script」→ 自动回填 editor + timeoutMs + note
* 「导出 JSON」按钮把当前 batch 完整结果下载为 `probe-<batchId>.json`

## 5. 回滚步骤

1. **临时关闭**：`PROBE_ENABLED=false` 重启 → 所有 admin probe 端点 503，sidebar 入口失效，客户端不再收 ProbeRequest。已存 row 不动。
2. **完全卸载**（不建议）：从 `migrate.rs` 移除 `m014_probe_executions` 注册（**仅未升级过的 DB 安全**）；prod 已升级过的 DB 表保留无碍。

## 6. 故障排查

| 症状 | 可能原因 | 排查 |
|---|---|---|
| dispatch 返回 503 PROBE_DISABLED | enabled=false | 检查 env / 重启 |
| dispatch 返回 429 RATE_LIMITED | per-admin 超限 | 等 1 分钟 / 调 PROBE_RATE_LIMIT_PER_MIN |
| 所有 device 都进 `skippedOffline` | 客户端 SSE 未连或被 ban | `/admin/clients` 看 sseLive 列表 |
| confirm 后客户端无响应 | confirm cache TTL 60s 过期 | 重新 dispatch；或客户端浏览器刷新过 |
| result_json 含 `_truncated_raw` | 结果超 256KB | 拆 script 或在 script 内自行裁剪 |
| status=unsupported_ctx_version | 客户端 CLIENT_CTX_VERSION 落后于后端 | 让客户端升级前端版本 |

## 7. 设计文档对应章节

* 协议契约：[设计稿 §3](../superpowers/specs/2026-05-19-remote-probe-design.md#3-协议契约)
* 表结构：[§4](../superpowers/specs/2026-05-19-remote-probe-design.md#4-表结构)
* Worker 沙箱：[§5](../superpowers/specs/2026-05-19-remote-probe-design.md#5-客户端-worker-沙箱)
* ctx schema：[§6](../superpowers/specs/2026-05-19-remote-probe-design.md#6-ctx-白名单-schemav1)
* 安全闸：[§7](../superpowers/specs/2026-05-19-remote-probe-design.md#7-安全闸)
