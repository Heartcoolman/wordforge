# Runbook：数据库备份与灾难恢复

> 适用：自托管 WordForge 生产环境（Linux x86_64 / aarch64，systemd + SQLite WAL）

---

## 概述

WordForge 使用 **SQLite WAL 模式**单库。备份机制有两层：

1. **自动备份**：自更新（`apply`）触发前自动调用 `Store::backup_to`（`PRAGMA wal_checkpoint(TRUNCATE)` + `VACUUM INTO`），产物为 `data/learning-{old_tag}.backup.db`，默认保留最近 3 份。
2. **手动备份**：运维在迁移 / 维护窗口期手动触发。

---

## 手动备份

### 症状
- 即将做重大配置变更、迁移服务器、或 schema 升级前

### 诊断
确认服务正在运行且 WAL 文件存在：

```bash
ls -lh /opt/wordforge/data/
# 应看到：learning.db  learning.db-wal  learning.db-shm
```

### 处置

```bash
INSTALL_DIR=/opt/wordforge
BACKUP_NAME="learning-manual-$(date +%Y%m%d-%H%M%S).backup.db"

# 方式一：在线 VACUUM INTO（推荐，不停机）
sqlite3 "$INSTALL_DIR/data/learning.db" \
  "PRAGMA wal_checkpoint(TRUNCATE); VACUUM INTO '$INSTALL_DIR/data/$BACKUP_NAME';"

# 验证备份完整性
sqlite3 "$INSTALL_DIR/data/$BACKUP_NAME" "PRAGMA integrity_check;" | head -5
# 期望输出：ok
```

> **注意**：`VACUUM INTO` 会短暂持有写锁（约 1–5 秒），期间新的写请求会等待（`busy_timeout` 默认 5000 ms）。

### 异机外推（推荐）

目前自动备份落在**同一磁盘**，机器丢失则全丢。强烈建议定期推到异机：

```bash
# rsync 到备份服务器（每日 cron 示例）
rsync -az "$INSTALL_DIR/data/learning-*.backup.db" backup-server:/backup/wordforge/

# 或推 S3（需 aws-cli）
aws s3 cp "$INSTALL_DIR/data/$BACKUP_NAME" s3://your-bucket/wordforge/backups/
```

---

## 从备份恢复

### 症状
- 自更新失败后服务无法启动，且 `wordforge.{old_tag}` 回滚也无效
- 数据库损坏（`PRAGMA integrity_check` 返回非 `ok`）
- 人为误操作导致数据丢失

### 诊断

```bash
INSTALL_DIR=/opt/wordforge

# 1. 检查现有备份列表（mtime 最新在前）
ls -lt "$INSTALL_DIR/data/learning-*.backup.db" 2>/dev/null

# 2. 校验目标备份完整性
BACKUP="$INSTALL_DIR/data/learning-vOLD.backup.db"
sqlite3 "$BACKUP" "PRAGMA integrity_check;" | head -3
```

### 处置

```bash
INSTALL_DIR=/opt/wordforge

# 1. 停服务
systemctl stop wordforge

# 2. 保留现场
mv "$INSTALL_DIR/data/learning.db" "$INSTALL_DIR/data/learning.db.broken"
mv "$INSTALL_DIR/data/learning.db-wal" "$INSTALL_DIR/data/learning.db-wal.broken" 2>/dev/null || true
mv "$INSTALL_DIR/data/learning.db-shm" "$INSTALL_DIR/data/learning.db-shm.broken" 2>/dev/null || true

# 3. 恢复最近可用备份
BACKUP=$(ls -t "$INSTALL_DIR/data/learning-*.backup.db" | head -1)
cp "$BACKUP" "$INSTALL_DIR/data/learning.db"

# 4. 验证
sqlite3 "$INSTALL_DIR/data/learning.db" "PRAGMA integrity_check;" | head -3

# 5. 重启
systemctl start wordforge
curl -sf http://127.0.0.1:3000/health
```

> **schema 降级警告**：若从高版本 binary 恢复到低版本 binary，旧 binary 可能无法读取高版本 migration 后的 schema。此时必须同步回滚 binary（见 `docs/auto-update.md` 手动回滚章节）。

---

## 定期备份 cron 示例

```bash
# /etc/cron.d/wordforge-backup
# 每天凌晨 3 点备份，保留最近 7 份
0 3 * * * wordforge bash -c '
  INSTALL_DIR=/opt/wordforge
  BACKUP="$INSTALL_DIR/data/learning-daily-$(date +%Y%m%d).backup.db"
  sqlite3 "$INSTALL_DIR/data/learning.db" \
    "PRAGMA wal_checkpoint(TRUNCATE); VACUUM INTO '"'"'$BACKUP'"'"';"
  ls -t "$INSTALL_DIR/data/learning-daily-*.backup.db" | tail -n +8 | xargs rm -f
'
```

---

## WAL 文件膨胀处理

### 症状
`learning.db-wal` 持续增长超过 100 MB，`PRAGMA wal_checkpoint(PASSIVE)` 返回未完成页数不归零。

### 诊断

```bash
# 查看 WAL 大小
ls -lh /opt/wordforge/data/learning.db-wal

# 查看 checkpoint 状态（busy_count, log_pages, checkpointed_pages）
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(PASSIVE);"
```

### 处置

```bash
# 强制 checkpoint（会阻塞写操作约数秒）
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(TRUNCATE);"
```

若 WAL 仍无法收缩（有长事务持锁），重启服务后再试：

```bash
systemctl restart wordforge
sqlite3 /opt/wordforge/data/learning.db "PRAGMA wal_checkpoint(TRUNCATE);"
```
