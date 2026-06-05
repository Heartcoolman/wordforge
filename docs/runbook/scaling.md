# 容量规划与扩容信号

本文描述 WordForge 单实例 SQLite 架构的容量基线、预警阈值，以及何时应考虑切换到 PostgreSQL。

## 当前容量基线（M0）

| 参数 | 值 | 来源 |
|------|----|------|
| SQLite 连接池 | 8（`SQLITE_POOL_SIZE=8`，v1.1.3-N1 自 16 下调以收敛峰值内存） | commit `2b80575`，M0-P2 |
| WAL 模式并发 | 1 writer + 15 readers | WAL 协议限制 |
| 写入理论上限 | ≈ 300 写/s（fsync NORMAL 限制） | `docs/v1-research/03-perf-warden.md §4.1` |
| 读取理论上限 | ≈ 10k+ 读/s（依赖 OS page cache + mmap 命中，工作集偏大或冷启动时余量收窄） | cache ≈15.6 MiB（-16000）+ mmap 128 MiB（每连接，v1.1.2-beta.3 防 OOM 收紧）；内存账 cache ≈15.6 MiB × pool 8 ≈ 125 MiB |
| SSE 连接上限 | 5000（`max_sse_connections`，v1.1-P2.4 自 1000 上调） | `src/config.rs LimitsConfig` |
| 单实例稳态 QPS 目标 | ≥ 100 req/s | perf-warden SLA §7.2 |
| 单实例峰值 QPS 目标 | ≥ 300 req/s | perf-warden SLA §7.2 |
| DB 大小 SLA | < 5 GiB | perf-warden SLA §7.2 |
| monitoring_events 保留 | 30 天滚动删除 | M0-P3，每月 1 日 UTC 03:00 VACUUM |

## 预警阈值

下列任意一条触发时，应评估是否需要扩容或架构升级。

### P0：立即响应

| 信号 | 阈值 | 诊断入口 |
|------|------|----------|
| DB 文件大小 | > 5 GiB | `GET /api/admin/monitoring/database` → `sizeOnDisk` |
| 5xx 错误率（滚动 1 分钟） | > 1% | `GET /metrics` → `http_5xx / http_requests` |
| SSE 连接数 | > 4500（接近 5000 上限） | `GET /metrics` → `sse_active_connections` |
| 连接池耗尽（`SQLITE_BUSY_TIMEOUT_MS` 频繁触发） | 日志出现 `pool error` > 10 次/分钟 | `journalctl -u wordforge --since "1 hour ago" \| grep "pool error"` |

### P1：当班评估

| 信号 | 阈值 | 含义 |
|------|------|------|
| 稳态 QPS | 持续 > 200 req/s | 接近峰值上限，写并发开始排队 |
| DB 文件大小 | > 2 GiB | 距 5 GiB 预警不足 1 年增长余量 |
| WAL 文件大小 | > 500 MiB | checkpoint 频率不足，考虑手动 `PRAGMA wal_checkpoint(TRUNCATE)` |
| AMAS process-event P95 | > 50 ms | 算法层出现瓶颈 |

### 读取 DB 大小

```bash
# 方式一：API（推荐，不登录服务器）
curl -s -H "Authorization: Bearer $ADMIN_TOKEN" \
  https://your-domain/api/admin/monitoring/database \
  | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(f'{d[\"sizeOnDisk\"]/1024**3:.2f} GiB')"

# 方式二：直接查文件
du -sh /opt/wordforge/data/learning.db
```

### 读取当前 QPS 估算

```bash
# 从 /metrics 计算近 5 分钟请求速率（需两次采样）
T1=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" https://your-domain/metrics \
  | grep '^http_requests ' | awk '{print $2}')
sleep 300
T2=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" https://your-domain/metrics \
  | grep '^http_requests ' | awk '{print $2}')
echo "QPS ≈ $(echo "($T2 - $T1) / 300" | bc -l | xargs printf '%.1f')"
```

## 容量增长估算

按假设 1 万活跃用户 × 500 学习记录/月 × 1 KB/记录：

| 活跃用户 | 月增长 | 年增长 |
|----------|--------|--------|
| 1,000 | ~0.5 GiB | ~6 GiB |
| 5,000 | ~2.5 GiB | ~30 GiB |
| 10,000 | ~5 GiB | ~60 GiB |

> 1,000 用户以上规模约 1 年内触碰 5 GiB 预警线。

## 短期扩容手段（不换数据库）

按优先级排序：

1. **提高 `SQLITE_POOL_SIZE`**：当前默认 8（v1.1.3-N1 自 16 下调以收敛峰值内存 = 每连接 cache × pool）。上限受 WAL 单 writer 约束，读密集场景可上调（如 16/32），但须同步评估峰值内存。写瓶颈不受影响。
2. **月度 VACUUM**：M0-P3 已实装（每月 1 日 UTC 03:00），确保 `ENABLE_ENGINE_MONITORING_WORKER=true`。
3. **扩磁盘**：阿里云 ECS 云盘可在线扩容，无需停机（`/opt/wordforge` 挂载点扩容后 `resize2fs`）。
4. **增加备份频率**：磁盘压力上升时适当提高 `VACUUM INTO` 备份频率，防止碎片累积。

## 何时考虑切换 PostgreSQL

满足以下**任意两条**时，应启动 PostgreSQL 迁移评估：

| 条件 | 说明 |
|------|------|
| DB 大小持续 > 5 GiB | SQLite VACUUM 开始痛苦，迁移窗口成本低 |
| 写 QPS 持续 > 200/s | 单 writer 瓶颈开始显现，P95 写延迟 > 20 ms |
| 活跃并发用户 > 5,000 | SSE + 写并发开始争抢连接池 |
| 需要跨进程写入（水平扩展） | SQLite 无法多进程并发写 |
| 需要流式复制 / 只读副本 | 读写分离需求出现 |

切换 PostgreSQL 不在 M0/M1 范围内，届时参考 RFC §9.2 制定迁移方案。

## 内核 / nginx 抗突发参数（v1.1.3-N3）

2026-06-01 压测在 ~300 req/s 突发下实测两处默认值偏低，且换机重装会回退默认，故固化进部署。两项均**不提升 goodput**（CPU 瓶颈），目的是消除 TIME-WAIT 溢出 / 连接上限导致的稳定性抖动。

### 1. sysctl：TIME-WAIT 桶上限

默认 `tcp_max_tw_buckets=5000` 在突发下被打爆（`netstat -s` 中 `TcpExtTW` 攀至 162797 并持续溢出），表现为连接异常。配合 nginx keepalive（见 `deploy/nginx/*.conf`，减少 TIME-WAIT 产生量）后，本参数兜底突发：

```bash
sudo cp deploy/sysctl.d/99-wordforge.conf /etc/sysctl.d/
sudo sysctl --system                      # 立即生效并在重启后持久
sysctl net.ipv4.tcp_max_tw_buckets        # 校验应为 262144
```

### 2. nginx：worker_connections

默认 `worker_connections 768` 在高并发 + keepalive 上游池下偏低。该指令位于 nginx **全局 `events{}` 块**（不在 `sites-available` 的 server 块内，故不能写进 `deploy/nginx/*.conf` 样例），需改 `/etc/nginx/nginx.conf`：

```nginx
events {
    worker_connections 4096;   # 默认 768；最大并发 ≈ worker_processes × worker_connections
}
```

改后执行 `nginx -t && systemctl reload nginx`。
