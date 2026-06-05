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

> 5xx 告警保留 **counter** 方案（`http_requests` / `http_5xx`）；路由级延迟 histogram（`http_request_duration_seconds{method,route,status}`）已随 M0-P1 实装，可直接用 `histogram_quantile()` 计算分位延迟。

| Metric 名 | 类型 | 说明 |
|-----------|------|------|
| `sse_active_connections` | gauge | 当前活跃 SSE 长连接数（raw socket 级别） |
| `sse_active_devices` | gauge | 持有 SSE 连接的唯一设备数 |
| `db_size_bytes` | gauge | SQLite 数据库文件字节数（page_count × page_size） |
| `http_requests` | counter | 进入 request_id_middleware 的 HTTP 请求总数（不含健康检查） |
| `http_5xx` | counter | 产生 5xx 响应的 HTTP 请求总数 |
| `http_inflight_requests` | gauge | 当前在途（正在处理中）的 HTTP 请求数——过载饱和度信号 |
| `http_request_duration_seconds{method,route,status}` | histogram | 路由级请求延迟直方图（按方法、路由、响应状态分类） |
| `worker_last_run_seconds{name}` | gauge | 各后台 worker 上次完成时的 Unix 时间戳（秒），0 表示尚未运行 |
| `amas_process_event_calls{algorithm}` | counter | AMAS 算法处理事件的累计调用次数 |
| `amas_process_event_errors{algorithm}` | counter | AMAS 算法处理事件时发生错误的累计次数 |
| `amas_process_event_latency_us{algorithm}` | counter | AMAS 算法处理事件的累计延迟（微秒），用于计算平均延迟 |
| `maintenance_mode_active` | gauge | 维护模式是否启用（1=是，0=否） |

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
      # 此处用 counter 差值近似；如需精确分位延迟改用 http_request_duration_seconds histogram
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

      # 在途请求持续堆积但 5xx=0 的过载盲区（饱和度信号）
      # 弥补「5xx=0 即判健康」的盲点：请求未失败但已积压，吞吐已饱和
      - alert: InflightRequestsSaturated
        expr: |
          http_inflight_requests{job="wordforge"} > 200
          and
          increase(http_5xx{job="wordforge"}[5m]) == 0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "WordForge 在途请求持续堆积（过载饱和）"
          description: "in-flight 请求 {{ $value }} 持续 5 分钟高于阈值且无 5xx，可能已达吞吐上限请求开始积压，请检查 CPU/连接池/慢查询。"

      # SSE 连接数接近上限（v1.1-P2.4：硬限 5000，预警 4000）
      - alert: SSEConnectionsHigh
        expr: sse_active_connections{job="wordforge"} > 4000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "SSE 连接数接近上限"
          description: "当前 SSE 连接 {{ $value }}，接近硬限 5000，考虑分流或提高上限。"

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
# HELP http_inflight_requests 当前在途（正在处理中）的 HTTP 请求数——过载饱和度信号
# TYPE http_inflight_requests gauge
http_inflight_requests 0
# HELP amas_process_event_calls AMAS 算法处理事件的累计调用次数
# TYPE amas_process_event_calls counter
amas_process_event_calls_total{algorithm="amas"} 1024
# HELP amas_process_event_errors AMAS 算法处理事件时发生错误的累计次数
# TYPE amas_process_event_errors counter
amas_process_event_errors_total{algorithm="amas"} 0
# HELP amas_process_event_latency_us AMAS 算法处理事件的累计延迟（微秒），用于计算平均延迟
# TYPE amas_process_event_latency_us counter
amas_process_event_latency_us_total{algorithm="amas"} 512000
# HELP maintenance_mode_active 维护模式是否启用（1=是，0=否）
# TYPE maintenance_mode_active gauge
maintenance_mode_active 0
# HELP http_request_duration_seconds HTTP 请求延迟直方图（秒），按路由、方法、响应状态分类
# TYPE http_request_duration_seconds histogram
http_request_duration_seconds_bucket{method="GET",route="/api/learning/next",status="200",le="0.01"} 12
http_request_duration_seconds_bucket{method="GET",route="/api/learning/next",status="200",le="+Inf"} 18
http_request_duration_seconds_count{method="GET",route="/api/learning/next",status="200"} 18
http_request_duration_seconds_sum{method="GET",route="/api/learning/next",status="200"} 0.21
# HELP worker_last_run_seconds 每个 worker 上次完成时的 Unix 时间戳（秒），0 表示尚未运行
# TYPE worker_last_run_seconds gauge
worker_last_run_seconds{name="monitoring_retention"} 1717200000
```

> AMAS 三条 counter 与 `http_request_duration_seconds` 直方图均含 label（`algorithm` / `method,route,status`），实际行数随活跃算法与命中路由数量展开；上方仅为单条示例。`http_request_duration_seconds_bucket` 的 `le` 取自固定桶边界，无数据时输出全零占位行以便 scraper 发现 metric 名。`worker_last_run_seconds` 每个 worker 一行，值为 Unix 时间戳。

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
`http_requests` / `http_5xx` 为内存 counter，进程重启归零。Prometheus `increase()` 函数对 counter 重置有内建处理（`rate()` 同），不影响告警准确性。`http_request_duration_seconds` histogram 同样为进程内累积，重启清零，行为一致。

**Q: 如何确认 monitoring_events retention 在运行**  
```bash
journalctl -u wordforge | grep "monitoring_retention"
# 期望看到：monitoring_retention: deleted old events / VACUUM 完成
```
每月 1 日 UTC 03:00 执行，首次部署后第一个月初才会有日志。
