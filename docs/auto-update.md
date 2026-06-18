# 后端自更新（GitHub Releases · 仅 Linux）

> 让管理员在 `/admin/updates` 看到红点 → 点"一键更新" → 后端自下载 + sha256 校验 + minisign 验签 + 替换二进制和 `static/` + fork-exec 自重启。
> 仅 Linux x86_64 / aarch64；不依赖 systemd。

---

## 整体数据流

```
┌────────────────────────────────────────────────────────────────┐
│ GitHub Releases (Heartcoolman/wordforge)                       │
│   /releases?per_page=10 → 列表分流双通道                        │
│   stable_latest = prerelease=false 最高 semver                 │
│   beta_latest   = 全部 releases 最高 semver（含 prerelease）    │
└──────────────────────────┬─────────────────────────────────────┘
                           │ ETag-aware GET（60/h 匿名，5000/h 带 token）
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ Worker: update_checker（cron `0 0 */1 * * *`，每小时整点）      │
│   → Updater::check_latest()                                    │
│   → stable / beta 各自比对 prev / new → SSE 广播               │
└──────────────────────────┬─────────────────────────────────────┘
                           ▼
            ┌─────────────────────────────┐
            │ 前端 /admin/updates 红点    │
            │ Stable 主卡 + Beta 折叠区   │
            │ 各自「一键更新到 vX.Y.Z」   │
            └───────────────┬─────────────┘
                            ▼
   ┌────────────────────────────────────────────────────────────┐
   │ POST /api/admin/updates/apply（立即返回 202 + taskId）      │
   │   请求体：{ channel, targetVersion, confirmCurrentVersion } │
   │   后台异步执行（v0.5.2+，避免 HTTP 499 中断升级流程）        │
   └────────────────────────┬───────────────────────────────────┘
                            ▼
   ┌────────────────────────────────────────────────────────────┐
   │ 前端轮询 GET /api/admin/updates/status → applyTask 字段    │
   │   phase: pending → downloading → verifying → extracting   │
   │         → backing_up_db → swapping → restarting → done    │
   └────────────────────────┬───────────────────────────────────┘
                            ▼
   ┌────────────────────────────────────────────────────────────┐
   │ apply 内部步骤（apply_locked 执行，每步有 5 分钟 watchdog）  │
   │   1. 下载 sha256 文件                                      │
   │   2. 流式下载 .tar.gz，同步计算 sha256                      │
   │   3. sha256 比对（不匹配 → SHA256_MISMATCH 422）            │
   │   4. minisign 验签（生产构建强制；本地构建跳过）              │
   │   5. tar 解压到 .update-staging/（zip-slip 守门）           │
   │   6. Store::backup_to → VACUUM INTO data/learning-vOLD.backup.db │
   │   7. Swapping：写 .maintenance.flag + 开启维护模式          │
   │   8. mv wordforge → wordforge.{old_tag}                   │
   │      mv static    → static.{old_tag}                      │
   │   9. mv staging/wordforge → wordforge                      │
   │      mv staging/static    → static                        │
   │  10. fork-exec 新进程（setsid，脱离 tty）                   │
   │  11. 父进程轮询新进程 /health 最多 60 秒                    │
   │  12. /health 通过 → exit(0)；失败 → 回滚二进制 + 关维护     │
   └────────────────────────────────────────────────────────────┘
```

任何步骤 8-9 中途失败 → 自动 rollback 已发生的 rename。fork-exec 后父进程等待子进程健康自检通过后退出，子进程拿到端口继续服务。

---

## 双通道（Stable / Beta）

v0.6.0-beta.3 起支持 stable / beta 双通道并存：

| 通道 | 定义 | 前端展示 |
|------|------|----------|
| Stable | `prerelease=false` 的最高 semver | 主卡（始终显示） |
| Beta | 所有 releases 的最高 semver（含 prerelease） | 折叠区（有 beta 时展示） |

后端通过单次 `/releases?per_page=10` 列表调用同时分流两个通道，**禁止**使用 `/releases/latest`（该端点跳过所有 prerelease）。

### 双通道 API 契约

`GET /api/admin/updates/status` 与 `POST /api/admin/updates/check` 共用以下响应结构：

```ts
interface AdminUpdateStatus {
  currentVersion: string;
  stable: ChannelStatus | null;       // stable 通道最新 release
  beta: ChannelStatus | null;         // beta 通道最新 release（null 表示无 beta）
  lastCheckedAt: string | null;
  autoCheckEnabled: boolean;
  allowDowngrade: boolean;
  applyTask?: ApplyTaskStatus;        // v0.5.2+ 后台任务进度
}

interface ChannelStatus {
  latestVersion: string;
  latestPublishedAt: string | null;
  releaseNotes: string;
  releaseUrl: string;
  hasUpdate: boolean;
  canApply: boolean;                  // 架构匹配 + 找到 tar.gz / sha256 资产对
}
```

---

## 异步 apply + applyTask 轮询

v0.5.2 起 apply 不阻塞 HTTP handler，避免前端 fetch 超时（HTTP 499）中断升级流程。

### 发起升级

```
POST /api/admin/updates/apply
Authorization: Bearer <ADMIN_TOKEN>
Content-Type: application/json

{
  "channel": "stable",           // 或 "beta"
  "targetVersion": "v1.2.1",
  "confirmCurrentVersion": "v1.2.0"   // 防误操作二次确认，必须与服务端 current 一致
}
```

立即返回 **202 Accepted**（`ApplyAccepted`）：

```json
{
  "data": {
    "taskId": "uuid-xxxx",
    "phase": "pending",
    "percent": 0,
    "targetVersion": "v1.2.1",
    "startedAt": "2026-05-21T03:00:00Z"
  }
}
```

### 轮询进度

前端每隔几秒调用 `GET /api/admin/updates/status`，读取响应中的 `applyTask` 字段：

```ts
interface ApplyTaskStatus {
  taskId: string;
  phase: string;      // 见下表
  percent: number;    // 0-100
  targetVersion: string;
  startedAt: string;
  completedAt?: string;
  error?: string;     // 仅 phase=failed 时有值
}
```

| phase | 含义 |
|-------|------|
| `pending` | 任务已创建，尚未开始 |
| `downloading` | 流式下载 .tar.gz |
| `verifying` | sha256 + minisign 验签 |
| `extracting` | 解压 tar.gz 到 staging |
| `backing_up_db` | VACUUM INTO 备份数据库 |
| `swapping` | 原子替换二进制和 static/（维护模式开启） |
| `restarting` | fork-exec 新进程 + 父进程健康自检（最多 60 秒） |
| `completed` | 升级成功（理论上进程已 exit，此状态为保底） |
| `failed` | 失败（见 `error` 字段） |

---

## minisign 验签（M0-R2）

每个 release 除 tar.gz / sha256 外新增一个 `.tar.gz.minisig` 资产：

```
wordforge-linux-x86_64.tar.gz
wordforge-linux-x86_64.tar.gz.sha256
wordforge-linux-x86_64.tar.gz.minisig   ← 新增
wordforge-linux-aarch64.tar.gz
wordforge-linux-aarch64.tar.gz.sha256
wordforge-linux-aarch64.tar.gz.minisig  ← 新增
```

公钥通过 `build.rs` 在编译期读取环境变量 `MINISIGN_PUBKEY` 并嵌入二进制（`env!("MINISIGN_PUBKEY")`）。

验签策略：

| 场景 | 行为 |
|------|------|
| 本地开发（`MINISIGN_PUBKEY` 为空） | warn 日志，跳过验签，不阻断 apply |
| 生产构建（`MINISIGN_PUBKEY` 非空）+ `.minisig` 资产存在 | 验签，不通过则 `SignatureInvalid` 错误 |
| 生产构建 + `.minisig` 资产**不存在**（`sig_url` 为空） | **强制拒绝**，防止降级攻击（攻击者控制 GitHub API 响应去掉 sig 资产 URL） |

### 私钥泄露应急

私钥泄露后必须立即处理，参考 `docs/runbook/key-rotation.md` 的 "minisign 私钥泄露应急" 章节。

---

## phase 超时（M0-P5）

每个 apply phase 独立受 **5 分钟（300 秒）watchdog** 保护：

- 超过 300 秒未推进到下一 phase → 强制 abort + 回滚
- 返回错误码 `PhaseTimeout { phase, timeout_secs }`
- 避免网络慢或下载卡住导致进程长期挂起

---

## maintenance 模式（M0-R4）

apply 进入 `Swapping` 阶段时自动开启维护模式，同时写 `.maintenance.flag` 文件到 install 目录：

- **维护模式开启**：阻止用户写入（防数据一致性问题），admin 后台仍可访问
- **成功路径**：进程 exit(0)，新进程启动时检测到 `.maintenance.flag` 文件后自动清理并关闭维护模式
- **失败/回滚路径**：删除 flag 文件 + 调用 `on_maintenance(false)` 恢复正常

> 若自更新中途崩溃后手动启动新进程，新进程会自动检测并清理残留的 `.maintenance.flag`，无需手动干预。

---

## 三档运营模式

| 模式 | 触发 | 行为 |
|------|------|------|
| 自动检查 + 手动一键更新（默认） | worker 每小时打 GitHub | 红点提醒，管理员点按钮才下载安装 |
| 仅手动检查 | `ENABLE_UPDATE_CHECKER_WORKER=false` | 进入 `/admin/updates` 点"立即检查"才打网络 |
| 完全禁用 | `UPDATE_CHECK_API_URL=`（空） | 不打网络，仅展示当前版本 |

---

## 环境变量

```dotenv
# 必须使用 /releases?per_page=N 列表端点（禁止 /releases/latest，会跳过 prerelease）
UPDATE_CHECK_API_URL=https://api.github.com/repos/Heartcoolman/wordforge/releases?per_page=10
UPDATE_CHECK_CACHE_TTL_SECS=3600
ENABLE_UPDATE_CHECKER_WORKER=true
UPDATE_CHECKER_INTERVAL_SECS=3600   # 目前 cron 写死为整点，此变量暂不生效
WORDFORGE_GITHUB_TOKEN=             # 可选；不填走 60/h 匿名限额，填后 5000/h
UPDATE_ALLOW_DOWNGRADE=false        # 仅用于灰度回滚
UPDATE_INSTALL_DIR=                 # 默认 current_exe 父目录
UPDATE_MAX_TARBALL_BYTES=209715200  # 200 MiB，流下载途中超限即拒
# 国内服务器到 GitHub release CDN 下载慢，可填镜像加速前缀
# GITHUB_DOWNLOAD_MIRROR_PREFIX=https://gh-proxy.com
```

---

## 安全网

| 守门 | 实现 |
|------|------|
| 并发更新 | `${install_dir}/.update.lock` 排他文件锁；已有运行中 task 返回 `409 UPDATE_IN_PROGRESS` |
| 误操作 | `confirmCurrentVersion` 必须 == 服务端 `currentVersion` |
| 通道错配 | `targetVersion` 必须 == 该通道缓存的 `latestVersion`，否则 `TargetMismatch` |
| Downgrade | 默认拒绝，要求 `UPDATE_ALLOW_DOWNGRADE=true` |
| 巨大产物 | `UPDATE_MAX_TARBALL_BYTES` 默认 200 MiB，流下载途中超限即拒 |
| sha256 | 流式下载同步计算，与 `.sha256` 文件比对，不匹配 `SHA256_MISMATCH` 422 |
| minisign | 生产构建强制验签（见上方"minisign 验签"章节） |
| Zip-slip | tar 条目含 `..` 或绝对路径直接拒；落地前严格 join 校验 |
| Symlink | tar 中任何 `Symlink` / `Link` 条目直接拒，避免 symlink 预置让后续 file 写出 dst 外 |
| 资产缺失 | apply 入口校验 `tarball_url` / `sha256_url` 非空，缺失立即 `NoAsset` |
| 数据库 | 备份到 `data/learning-{old_tag}.backup.db`，自动保留 3 份（按 mtime 删旧） |
| 旧二进制 | `wordforge.{old_tag}` + `static.{old_tag}/` 各保留 2 份 |
| phase 超时 | 每 phase 独立 300 秒 watchdog，超时强制 abort + 回滚 |

---

## 手动回滚（自更新出问题时）

```bash
cd /opt/wordforge          # 或实际 install 目录

# 1) 停止当前（已损坏的）进程
pkill -f wordforge || true

# 2) 清理维护模式 flag（若残留）
rm -f .maintenance.flag

# 3) 确认可用的旧版本备份
ls -lt wordforge.v* 2>/dev/null | head -5
ls -ltd static.v* 2>/dev/null | head -5

# 4) 恢复二进制和 static/
mv wordforge wordforge.broken 2>/dev/null || true
mv static static.broken 2>/dev/null || true
last_bin=$(ls -t wordforge.v* 2>/dev/null | head -1)
last_static=$(ls -td static.v* 2>/dev/null | head -1)
mv "$last_bin" wordforge
mv "$last_static" static

# 5) 回滚 DB（仅当升级有破坏性 schema 变更时才需要）
ls data/learning-v*.backup.db
cp data/learning-v<OLD>.backup.db data/learning.db

# 6) 重启
./wordforge
# 或通过 systemd：systemctl restart wordforge
```

**注意**：DB schema 升级是单向的。回滚 binary 后，旧版本可能读不懂新 schema，所以**必须**先用 backup 文件恢复 DB（步骤 5）。

---

## 限流

- 不带 token：60 次/小时 / IP（GitHub 匿名）
- 带 `WORDFORGE_GITHUB_TOKEN`：5000 次/小时
- 命中 ETag 304 时**不计入** primary rate limit（前提是带 token）
- Worker 每小时检查 + 手动 `/check` 调用共享同一限额
- 超限返回 `503 GITHUB_RATE_LIMITED`

---

## 路由

| 路径 | 方法 | 作用 |
|------|------|------|
| `/api/admin/updates/status` | GET | 返回缓存内版本视图（含双通道 + `applyTask` 进度），不打网络 |
| `/api/admin/updates/check` | POST | 强制刷新（绕过 TTL，仍带 ETag，命中 304 时省额度）；stable/beta 各自广播 SSE |
| `/api/admin/updates/apply` | POST | 立即返回 202 + taskId，后台异步执行完整自更新流程 |

---

## SSE 事件

| event name | payload | 谁发 |
|------------|---------|------|
| `release_available` | `{ latestTag, channel }` | worker / `/check` 端点发现通道有更新时 |
| `update_progress` | `{ phase, percent }` | apply 各 phase 推送 |

`release_available` 从 v0.6.0-beta.3 起携带 `channel` 字段（`"stable"` / `"beta"`），前端据此刷新对应通道的红点。

---

## 验证

| 类别 | 命令 | 期望 |
|------|------|------|
| 后端单测 | `cargo test --test updater_http` | 通过 |
| 后端 lib | `cargo test --lib` | 通过 |
| 前端单测 | `pnpm -C admin-ui test -- tests/pages/admin/UpdatesPage.test.tsx` | 通过 |

---

## 已知约束

- **cron 写死为整点**：`UPDATE_CHECKER_INTERVAL_SECS` 暂不生效。要改频率请直接修改 `src/workers/mod.rs` 中 `WorkerName::UpdateChecker` 的 cron 字符串。
- **仅 Linux**：macOS / Windows 不会自更新；开发机上点"一键更新"会得到 `NO_ASSET`。
- **static/ 仍为外部目录**：更新中途崩溃可能出现 `wordforge` 和 `static/` 不同步，失败时自动 rollback。
- **`/health` 轮询为 HTTP**：fork-exec 后父进程通过 `http://127.0.0.1:{port}/health` 轮询新进程，健康自检 60 秒超时。
