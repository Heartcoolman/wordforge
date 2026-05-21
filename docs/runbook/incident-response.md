# Runbook：事故响应

> 适用：自托管 WordForge 生产环境
> 结构：每个场景按 **症状 → 诊断 → 处置** 三段式展开

---

## 场景一：5xx 错误率上涨

### 症状
- `GET /health` 返回非 200，或 HTTP 响应体显示错误
- 客户端大量 5xx
- admin SSE 收到 `incident` 告警事件（M0-P4 落地后）

### 诊断

```bash
# 1. 检查服务状态
systemctl status wordforge

# 2. 查最近错误日志（若启用文件日志）
tail -100 /opt/wordforge/logs/*.log | grep -E "ERROR|WARN|panic"

# 3. 健康端点
curl -sf http://127.0.0.1:3000/health
# 期望：{"status":"ok"} 或 {"status":"degraded",...}

# 4. 检查 DB 连接池是否耗尽（busy_timeout 超时会返回 5xx）
# 看日志里是否有 "pool timed out" / "database is locked"
journalctl -u wordforge -n 200 | grep -i "lock\|timeout\|pool"
```

### 处置

**情况 A：进程崩溃（服务不在）**

```bash
systemctl start wordforge
# 若 systemd 已拉起（Restart=always），确认进程在运行
systemctl status wordforge
```

**情况 B：DB 锁超时（写请求积压）**

```bash
# 查看是否有长事务持锁
sqlite3 /opt/wordforge/data/learning.db ".timeout 2000" "SELECT * FROM sqlite_master LIMIT 1;"

# 若 WAL 膨胀导致 checkpoint 阻塞
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(PASSIVE);"
```

**情况 C：内存/CPU 飙升**

```bash
# 查看进程资源
top -p $(pgrep wordforge)

# 若需立即止血：重启（会触发 systemd Restart=always 拉起）
systemctl restart wordforge
```

**情况 D：panic（tracing log 有 panic 字样）**

```bash
# 查完整 panic 信息
journalctl -u wordforge -n 500 | grep -A 20 "panic\|PANIC"

# 记录现场，重启
systemctl restart wordforge
```

---

## 场景二：SSE 连接打满

### 症状
- 客户端报"连接超出限制"或 SSE 断开后无法重连
- 日志出现 `SSE_CONNECTION_COUNT` 相关错误
- 全局 SSE 上限（默认 1000，由 `LIMITS_MAX_SSE_CONNECTIONS` 配置）被触发

### 诊断

```bash
# 查 SSE 相关日志
journalctl -u wordforge -n 200 | grep -i "sse\|connection_count\|limit"

# 查当前 SSE 连接数（如已部署 /metrics 端点，M0-P1 落地后）
curl -sf http://127.0.0.1:3000/metrics 2>/dev/null | grep sse_connections
```

### 处置

**短期止血**：重启服务强制断开所有现有 SSE 连接：

```bash
systemctl restart wordforge
```

**根本原因排查**：

1. 检查是否有客户端在断开后不停重连（bug 或攻击）：查 nginx access log，找高频 `GET /api/realtime/events` 的 IP。
2. 确认 `LIMITS_MAX_SSE_CONNECTIONS` 配置是否需要调高：

```bash
# 临时调高（重启生效）
grep LIMITS_MAX_SSE_CONNECTIONS /opt/wordforge/.env
# 默认 1000，根据服务器内存判断是否调整
```

3. 若是 heartbeat_watchdog 误触发 `data_corrupted`（见下方场景五），SSE 客户端会重连，可能堆积。

---

## 场景三：GitHub API Rate Limit 超额（自更新受阻）

### 症状
- admin 后台 Updates 页面显示 `GITHUB_RATE_LIMITED` 或 503
- 手动触发 `POST /api/admin/updates/check` 返回错误
- 日志出现 `rate_limit` 相关字样

### 诊断

```bash
# 直接测 GitHub API 剩余额度
curl -sf -H "Authorization: Bearer ${GITHUB_TOKEN}" \
  https://api.github.com/rate_limit | jq '.rate'
# 看 "remaining" 字段，0 则已超额；"reset" 字段为额度重置时间（Unix timestamp）

# 检查 .env 是否配了 token
grep WORDFORGE_GITHUB_TOKEN /opt/wordforge/.env
```

### 处置

**未配 token（匿名，60 次/小时）**：

```bash
# 生成 GitHub Personal Access Token（只需 public_repo 只读权限）
# 然后加入 .env
echo "WORDFORGE_GITHUB_TOKEN=ghp_xxxxxxxxxxxx" >> /opt/wordforge/.env
systemctl restart wordforge
```

**已配 token 但仍超额**：

- 默认每小时检查一次（cron），额度 5000 次/小时，正常不会超
- 检查是否有其他服务共用同一 token

**临时绕过**：等待 rate limit 重置（最多 1 小时），或手动下载 tarball 后走手动升级：

```bash
# 手动下载指定版本（不走 updater）
VERSION=v0.6.0
ARCH=x86_64
URL="https://github.com/Heartcoolman/wordforge/releases/download/${VERSION}/wordforge-linux-${ARCH}.tar.gz"
curl -L -o /tmp/wordforge.tar.gz "$URL"
# 然后按手动升级步骤操作
```

---

## 场景四：磁盘空间告警

### 症状
- `df -h` 显示挂载点使用率 > 90%
- 服务日志出现写入失败
- WAL 文件异常膨胀

### 诊断

```bash
INSTALL_DIR=/opt/wordforge

# 查各目录占用
du -sh "$INSTALL_DIR"/data/
du -sh "$INSTALL_DIR"/logs/ 2>/dev/null
du -sh "$INSTALL_DIR"/data/learning-*.backup.db 2>/dev/null | sort -h

# 查 WAL 文件大小
ls -lh "$INSTALL_DIR/data/learning.db-wal"
```

### 处置

**清理旧备份**（只保留最近 2 份）：

```bash
ls -t /opt/wordforge/data/learning-*.backup.db | tail -n +3 | xargs rm -f
```

**清理旧 binary / static 备份**：

```bash
# 保留最新 2 个旧版本（与 KEEP_OLD_VERSIONS=2 一致）
ls -t /opt/wordforge/wordforge.v* | tail -n +3 | xargs rm -f
ls -td /opt/wordforge/static.v* | tail -n +3 | xargs rm -rf
```

**收缩 WAL**：

```bash
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(TRUNCATE);"
```

**清理日志**（若启用了文件日志）：

```bash
find /opt/wordforge/logs/ -name "*.log" -mtime +30 -delete
```

**在线 VACUUM**（回收删除后的空页，需低峰期）：

```bash
# VACUUM 会短暂阻塞写入，建议在凌晨低峰期运行
sqlite3 /opt/wordforge/data/learning.db "VACUUM;"
```

---

## 场景五：WAL 文件不收缩（CHECKPOINT 未完成）

### 症状
- `learning.db-wal` 持续增长（> 50 MB 不正常）
- `PRAGMA wal_checkpoint(PASSIVE)` 返回 `busy_count > 0`，`checkpointed_pages < log_pages`

### 诊断

```bash
# 查 checkpoint 状态（返回三列：busy_count, log_pages, checkpointed_pages）
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(PASSIVE);"

# 若 busy_count > 0，说明有读连接持锁，阻塞 checkpoint
# 检查连接池日志
journalctl -u wordforge -n 100 | grep -i "pool\|lock\|busy"
```

### 处置

```bash
# 1. 等待读连接释放后用 TRUNCATE 强制完成（短暂写阻塞）
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(TRUNCATE);"

# 2. 若仍失败，重启服务后重试
systemctl restart wordforge
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(TRUNCATE);"
```

---

## 场景六：自更新失败后服务起不来（updater 回滚未还原 static）

### 症状
- 自更新完成后服务无法访问 admin 前端（空白页 / 404）
- `wordforge` binary 可能是新版，但 `static/` 目录仍是旧版（或已损坏）
- 日志出现 `static/` 相关路径错误

> **背景**：updater 替换流程是先 swap binary，再 swap static/。若 binary swap 成功但 static swap 失败，binary 和 static 会版本不一致。回滚机制还原的是 binary，**不自动还原 static/**。

### 诊断

```bash
INSTALL_DIR=/opt/wordforge

# 检查当前 binary 版本
"$INSTALL_DIR/wordforge" --version 2>/dev/null || \
  curl -sf http://127.0.0.1:3000/api/status | jq '.data.version'

# 检查 static/ 目录是否存在且完整
ls "$INSTALL_DIR/static/"
# 期望看到 index.html 等前端产物

# 检查是否有旧版 static 备份
ls -d "$INSTALL_DIR/static.v"*
```

### 处置

**方式 A：从旧版 static 备份恢复**

```bash
INSTALL_DIR=/opt/wordforge

# 停服
systemctl stop wordforge

# 找对应版本的 static 备份
OLD_STATIC=$(ls -td "$INSTALL_DIR/static.v"* | head -1)
echo "恢复: $OLD_STATIC"

# 备份当前（可能损坏的）static
mv "$INSTALL_DIR/static" "$INSTALL_DIR/static.broken"

# 恢复
cp -r "$OLD_STATIC" "$INSTALL_DIR/static"

# 重启
systemctl start wordforge
curl -sf http://127.0.0.1:3000/health
```

**方式 B：从 release tarball 提取 static/**

```bash
VERSION=$(curl -sf http://127.0.0.1:3000/api/status | jq -r '.data.version')
ARCH=x86_64
TARBALL="/tmp/wordforge-linux-${ARCH}.tar.gz"

# 下载对应版本 tarball
curl -L -o "$TARBALL" \
  "https://github.com/Heartcoolman/wordforge/releases/download/${VERSION}/wordforge-linux-${ARCH}.tar.gz"

# 提取 static/
tar xzf "$TARBALL" --strip-components=1 -C /tmp/wf-extract/ "wordforge-linux-${ARCH}/static"

# 替换
systemctl stop wordforge
mv /opt/wordforge/static /opt/wordforge/static.broken
mv /tmp/wf-extract/static /opt/wordforge/static
systemctl start wordforge
```

---

## 事故处置后检查清单

- [ ] `GET /health` 返回 200
- [ ] admin 后台可正常登录和访问
- [ ] `PRAGMA integrity_check` 返回 `ok`
- [ ] 日志无持续 ERROR/WARN 输出
- [ ] SSE 连接可正常建立
- [ ] 记录事故发生时间、根因、处置步骤（写入 incident log 或 GitHub Issue）
