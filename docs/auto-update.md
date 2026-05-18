# 后端自更新（GitHub Releases · 仅 Linux）

> 让管理员在 `/admin/updates` 看到红点 → 点"一键更新" → 后端自下载 + sha256 校验 + 替换二进制和 `static/` + fork-exec 自重启。
> 仅 Linux x86_64 / aarch64；不依赖 systemd。

---

## 数据流

```
┌────────────────────────────────────────────────────────────────┐
│ GitHub Releases (Heartcoolman/wordforge)                       │
│   • wordforge-linux-{x86_64,aarch64}.tar.gz                    │
│   • wordforge-linux-{x86_64,aarch64}.tar.gz.sha256             │
└──────────────────────────┬─────────────────────────────────────┘
                           │ ETag-aware GET (60/h anon, 5000/h with token)
                           ▼
┌────────────────────────────────────────────────────────────────┐
│ Worker: update_checker (cron `0 0 */1 * * *`)                  │
│   → Updater::check_latest()                                    │
│   → 新 tag 不同于上次缓存 → SseEvent::ReleaseAvailable          │
└──────────────────────────┬─────────────────────────────────────┘
                           ▼
            ┌─────────────────────────────┐
            │ 前端 /admin/updates 红点    │
            │ + Card「一键更新到 vX.Y.Z」 │
            └───────────────┬─────────────┘
                            ▼
   ┌────────────────────────────────────────────────────────────┐
   │ POST /api/admin/updates/apply                              │
   │   1. ETag/cache 已有 latest 元数据                          │
   │   2. fetch sha256 file (流式短小)                          │
   │   3. stream-download .tar.gz, 同步算 sha256                │
   │   4. 不匹配 → SHA256_MISMATCH 422                          │
   │   5. tar.gz 解压到 .update-staging/，zip-slip 守门          │
   │   6. Store::backup_to → VACUUM INTO data/learning-vX.Y.Z.backup.db │
   │   7. mv wordforge → wordforge.vOLD                         │
   │      mv static    → static.vOLD                            │
   │   8. mv staging/wordforge → wordforge                      │
   │      mv staging/static    → static                         │
   │   9. spawn 新二进制 (setsid, 脱离 tty)                      │
   │  10. exit(0)                                               │
   └────────────────────────────────────────────────────────────┘
```

任何步骤 7-9 中途失败 → 自动 rollback 已发生的 rename。fork-exec 后父进程立即退出，子进程拿到端口继续服务。

## 三档运营模式

| 模式 | 触发 | 行为 |
|---|---|---|
| 自动检查 + 手动一键更新（默认） | worker 每小时打 GitHub | 红点提醒，管理员点按钮才下载安装 |
| 仅手动检查 | `ENABLE_UPDATE_CHECKER_WORKER=false` | 进入 `/admin/updates` 点"立即检查"才打网络 |
| 完全禁用 | `UPDATE_CHECK_API_URL=`（空） | 不打网络，仅展示当前版本 |

## 环境变量

```dotenv
UPDATE_CHECK_API_URL=https://api.github.com/repos/Heartcoolman/wordforge/releases/latest
UPDATE_CHECK_CACHE_TTL_SECS=3600
ENABLE_UPDATE_CHECKER_WORKER=true
UPDATE_CHECKER_INTERVAL_SECS=3600          # 当前未生效，cron 写死为整点（详见下方"已知约束"）
WORDFORGE_GITHUB_TOKEN=                    # 可选；不填走 60/h 匿名限额
UPDATE_ALLOW_DOWNGRADE=false               # 仅用于灰度回滚
UPDATE_INSTALL_DIR=                        # 默认 current_exe 父目录
UPDATE_MAX_TARBALL_BYTES=209715200         # 200 MiB
```

## 验证产物（sha256）

`.github/workflows/release.yml` 的 `Package` 步骤在打完 tar.gz 之后追加：

```bash
sha256sum "${DIST}.tar.gz" | awk '{print $1}' > "${DIST}.tar.gz.sha256"
```

Release 资产共 4 个文件：

```
wordforge-linux-x86_64.tar.gz
wordforge-linux-x86_64.tar.gz.sha256
wordforge-linux-aarch64.tar.gz
wordforge-linux-aarch64.tar.gz.sha256
```

Updater 在 apply 时同时 stream 这两个 URL，不匹配立即拒绝，**不**信任 HTTPS 自身。

## 安全网

| 守门 | 实现 |
|---|---|
| 并发更新 | `${install_dir}/.update.lock` 排他文件锁；冲突返回 `409 UPDATE_IN_PROGRESS` |
| 误操作 | `confirmCurrentVersion` 必须 == 服务端 `GIT_VERSION` |
| Downgrade | 默认拒绝，要求 `allow_downgrade=true` |
| 巨大产物 | `max_tarball_bytes` 默认 200 MiB，流下载途中超限即拒 |
| Zip-slip | tar 条目里有 `..` 或绝对路径直接拒，落地前严格 join 校验 |
| 数据库 | 备份到 `data/learning-{old_tag}.backup.db`，自动保留 3 份按 mtime 删旧 |
| 旧二进制 | `wordforge.{old_tag}` + `static.{old_tag}/` 各保留 2 份 |

## 手动回滚（自更新出问题时）

```bash
cd /path/to/install/dir          # 例如 /opt/wordforge
# 1) 杀掉当前（已损坏的）进程
pkill -f wordforge

# 2) 把当前可执行重命名为 .broken（保留现场）
mv wordforge wordforge.broken
mv static static.broken

# 3) 恢复最近一份 backup（mtime 最大的那份）
last_good=$(ls -t wordforge.v* 2>/dev/null | head -1)
mv "$last_good" wordforge
last_good_static=$(ls -td static.v* 2>/dev/null | head -1)
mv "$last_good_static" static

# 4) （可选）回滚 DB —— 仅当确认本次升级在 schema 上有破坏性变更
ls data/learning-v*.backup.db
cp data/learning-vOLD.backup.db data/learning.db

# 5) 重新启动
./wordforge
```

**注意**：DB schema 升级是单向的。回滚 binary 后，旧版本可能读不懂新 schema。
所以**必须**先用 backup 文件恢复 DB（步骤 4）。本工程的 `migrate.rs` 是顺序 idempotent
迁移，不会在旧版本里跑高版本的 migration，但旧版本读不到高版本才有的列也是事实，因此 backup 是必备退路。

## 限流

- 不带 token：60 次/小时 / IP（GitHub 匿名）
- 带 `WORDFORGE_GITHUB_TOKEN`：5000 次/小时
- 命中 ETag 304 时**不计入** primary rate limit（前提是带 token）
- worker 每小时检查 + 手动 `/check` 调用都共享同一限额。超限返回 `503 GITHUB_RATE_LIMITED`。

## 已知约束

- **cron 写死为整点（hourly）**：`UPDATE_CHECKER_INTERVAL_SECS` 暂不生效。要改频率请直接改 `src/workers/mod.rs` 里的 `WorkerName::UpdateChecker` 的 cron 字符串。
- **不签名**：当前仅 sha256 校验，没有 minisign/cosign。GitHub 账号被入侵会让验证失效。如需补，参考 [minisign-verify](https://github.com/jedisct1/minisign) 在 `Updater::fetch_sha256` 之后追加签名校验。
- **不嵌入前端**：`static/` 仍是外部目录而非编译期 embed。好处是改动小、调试方便；代价是更新中途崩溃可能出现 `wordforge` 和 `static/` 不同步。失败时自动 rollback。
- **不跨平台**：macOS / Windows 不会自更新；开发机上点按钮会得到 `NO_ASSET`。

## 验证

| 类别 | 命令 | 期望 |
|---|---|---|
| 后端单测 | `cargo test --test updater_http` | 7 passed |
| 后端 lib | `cargo test --lib` | 224 passed |
| 前端 | `pnpm -C frontend test -- tests/pages/admin/UpdatesPage.test.tsx` | 5 passed |
| Mock 端到端 | 见 `tests/updater_http.rs::apply_runs_until_backup_callback_when_sha_matches` | sha256 流校验通过、staging 落地 |

## 路由

| 路径 | 方法 | 作用 |
|---|---|---|
| `/api/admin/updates/status` | GET | 返回缓存内的版本视图，不打网络 |
| `/api/admin/updates/check`  | POST | 强制刷新（带 ETag），命中 304 时省额度 |
| `/api/admin/updates/apply`  | POST | 触发完整自更新流程，成功后 fork-exec 退出 |
| `/api/admin/monitoring/check-update` | GET | **遗留**，前端 dashboard 仍在用，结构更简单 |

## SSE 事件

| event name | payload | 谁发 |
|---|---|---|
| `release_available` | `{ latestTag: "v0.4.3" }` | worker 探测到新版本时 / `/check` 端点强制刷新发现变化时 |
| `update_progress` | `{ phase: "downloading", percent: 35 }` | apply 阶段 6 次推送：downloading / verifying / extracting / backing_up_db / swapping / restarting |

前端在 `/admin/updates` 页订阅这两个事件，分别用来刷红点和驱动进度条。
