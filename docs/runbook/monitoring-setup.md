# 外部监控对接（Prometheus / Alertmanager）

## `/metrics` 端点说明

WordForge 在 `/metrics` 暴露 Prometheus OpenMetrics 文本格式（M0-P1 实装）。

**鉴权**：与 `/api/admin/*` 同等级，需要 Admin JWT Bearer Token。

```
GET /metrics
Authorization: Bearer <ADMIN_TOKEN>
Content-Type: text/plain; version=0.0.4; charset=utf-8
```

### M0 当前实装 Metric 列表

> M0 当前实装 **counter** 简化方案（`http_requests` / `http_5xx`）；M2-Q1 前升级为 histogram（`http_request_duration_seconds{route,method,status}`），届时同步更新 scrape 规则。

| Metric 名 | 类型 | 说明 |
|-----------|------|------|
| `sse_active_connections` | gauge | 当前活跃 SSE 长连接数（raw socket 级别） |
| `sse_active_devices` | gauge | 持有 SSE 连接的唯一设备数 |
| `db_size_bytes` | gauge | SQLite 数据库文件字节数（page_count × page_size） |
| `http_requests` | counter | 进入 request_id_middleware 的 HTTP 请求总数（不含健康检查） |
| `http_5xx` | counter | 产生 5xx 响应的 HTTP 请求总数 |
| `maintenance_mode_active` | gauge | 维护模式是否启用（1=是，0=否） |

M2-Q1 计划新增（当前未实装）：

| Metric 名 | 类型 | 说明 |
|-----------|------|------|
| `http_request_duration_seconds{route,method,status}` | histogram | 路由级请求延迟 |
| `worker_last_run_seconds{name}` | gauge | 各后台 worker 上次运行时间戳 |
| `amas_process_event_duration_seconds` | histogram | AMAS process-event P95 延迟 |

## Prometheus 配置示例

```yaml
# prometheus.yml（追加到 scrape_configs）
scrape_configs:
  - job_name: wordforge
    scrape_interval: 30s
    scrape_timeout: 10s
    scheme: https                       # 生产环境必须 HTTPS
    metrics_path: /metrics
    bearer_token: "<ADMIN_TOKEN>"       # 从 Alertmanager Secret 注入，勿明文硬编码
    static_configs:
      - targets:
          - "your-domain:443"           # 替换为实际域名或 IP:PORT
        labels:
          env: production
          service: wordforge
```

> 若 Prometheus 与 WordForge 同机部署，`targets` 可填 `localhost:3000`，`scheme` 改 `http`，并将 bearer_token 写入 `prometheus-secrets.yaml`（`kubectl create secret` 或 `ansible-vault`）。

## Alertmanager 规则示例

```yaml
# wordforge-alerts.yml
groups:
  - name: wordforge
    interval: 1m
    rules:
      # 5xx 错误率 > 1%（滚动 5 分钟）
      # M0 用 counter 差值近似；M2-Q1 升级 histogram 后改用 rate()
      - alert: HighErrorRate
        expr: |
          (
            increase(http_5xx{job="wordforge"}[5m]) /
            increase(http_requests{job="wordforge"}[5m])
          ) > 0.01
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "WordForge 5xx 错误率超过 1%"
          description: "过去 5 分钟 5xx 比率 {{ $value | humanizePercentage }}，请检查日志。"

      # SSE 连接数接近上限（硬限 1000，预警 800）
      - alert: SSEConnectionsHigh
        expr: sse_active_connections{job="wordforge"} > 800
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "SSE 连接数接近上限"
          description: "当前 SSE 连接 {{ $value }}，接近硬限 1000，考虑分流或提高上限。"

      # DB 大小超过 5 GiB
      - alert: DatabaseSizeHigh
        expr: db_size_bytes{job="wordforge"} > 5368709120
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "SQLite 数据库文件超过 5 GiB"
          description: "DB 大小 {{ $value | humanize1024 }}B，接近容量上限，参考 scaling.md 评估迁移。"

      # 维护模式异常开启（非计划内）
      - alert: MaintenanceModeUnexpected
        expr: maintenance_mode_active{job="wordforge"} == 1
        for: 15m
        labels:
          severity: info
        annotations:
          summary: "WordForge 维护模式持续开启超过 15 分钟"
          description: "请确认是否计划内维护，若非计划内请立即关闭。"
```

## 本地验证 `/metrics` 可达性

```bash
# 获取 Admin Token（有效期 2h，见 ADMIN_JWT_EXPIRES_IN_HOURS）
ADMIN_TOKEN=$(curl -s -X POST https://your-domain/api/admin/auth/login \
  -H "Content-Type: application/json" \
  -d '{"password":"<ADMIN_PASSWORD>"}' | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['token'])")

# 拉取 metrics
curl -s -H "Authorization: Bearer $ADMIN_TOKEN" https://your-domain/metrics
```

期望输出（M0 实装）：

```
# HELP sse_active_connections 当前活跃 SSE 长连接数（raw socket 级别）
# TYPE sse_active_connections gauge
sse_active_connections 0
# HELP sse_active_devices 当前持有 SSE 连接的唯一设备数
# TYPE sse_active_devices gauge
sse_active_devices 0
# HELP db_size_bytes SQLite 数据库文件字节数（page_count × page_size）
# TYPE db_size_bytes gauge
db_size_bytes 12345678
# HELP http_requests 进入 request_id_middleware 的 HTTP 请求总数（不含健康检查）
# TYPE http_requests counter
http_requests 42
# HELP http_5xx 产生 5xx 响应的 HTTP 请求总数
# TYPE http_5xx counter
http_5xx 0
# HELP maintenance_mode_active 维护模式是否启用（1=是，0=否）
# TYPE maintenance_mode_active gauge
maintenance_mode_active 0
```

## 无外部 Prometheus 时的替代方案

若暂不接入 Prometheus，可用 WordForge 内置的 Error Rate Watchdog（M0-P4）：

- 每分钟采样一次 `http_5xx / http_requests`
- 超过 1% 时通过 admin SSE 广播 `incident` 事件
- Admin 后台「监控」页面实时展示

此方案覆盖最关键的 5xx 告警，无需额外基础设施。

## 常见问题

**Q: Prometheus 拉取返回 401**  
Admin Token 有效期仅 2 小时（`ADMIN_JWT_EXPIRES_IN_HOURS=2`）。Prometheus 长期运行需要定期刷新 Token，建议通过脚本定时更新 `bearer_token_file` 或集成 Vault 动态 Secret。

**Q: `http_requests` 计数从 0 开始，重启后重置**  
当前 M0 实装为内存 counter，进程重启归零。Prometheus `increase()` 函数对 counter 重置有内建处理（`rate()` 同），不影响告警准确性。M2-Q1 升级 histogram 时保持此行为。

**Q: 如何确认 monitoring_events retention 在运行**  
```bash
journalctl -u wordforge | grep "monitoring_retention"
# 期望看到：monitoring_retention: deleted old events / VACUUM 完成
```
每月 1 日 UTC 03:00 执行，首次部署后第一个月初才会有日志。
