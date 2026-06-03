# 运维：HTTP 延迟直方图桶边界变更（W3-3）

## 变更内容

`http_request_duration_seconds` 的 `BUCKET_BOUNDS`（`src/middleware/http_metrics.rs`）由

```
[0.01, 0.05, 0.1, 0.5, 2.0]          # 6 桶（含 +Inf）
```

细化为

```
[0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5]   # 9 桶（含 +Inf）
```

**动机**：原方案在 100ms~2s 之间仅有 2 个桶，`histogram_quantile` 对 p95/p99 的线性插值在这段
严重失真——而慢请求恰是 SLO 关注点。细化后该区间有 4 个桶，p95/p99 精度显著提升。

## 发版窗口需注意

### 1. Prometheus 端 series 断层

`http_request_duration_seconds_bucket` 的 `le` 标签集在升级后变化（新增 `0.025/0.25/1.0/2.5`，
移除 `2.0`）。Prometheus 会把新旧 `le` 视为不同 series：

- 升级时间点前后，按 `le` 聚合的 `histogram_quantile` 查询会出现一次性的不连续。
- 跨升级窗口的 p99 趋势图会有一个断点，**属预期**，非数据丢失。

**处置**：在 Grafana 对应面板加一条 annotation 标注升级时间；或查询时用 `rate(...[5m])` 让旧
series 自然衰减。

### 2. 登录页 SLO 30d 持久化桶（availability_rollup）

`availability_rollup` 持久化每小时直方图桶 JSON。升级后启动回灌（`import_hour_rollup`）会遇到
**旧 6 桶行 vs 新 9 桶 schema** 的长度不符：

- 代码已处理（W3-3）：检测到桶向量长度 ≠ 新 schema 时，**丢弃该行直方图（置零）**，但保留
  `count`/`err5xx`——**可用率比率（基于 count）不受影响**，仅升级前那些小时桶的延迟分位失真
  被如实清零，不伪造历史。
- 即：升级后回看升级前 ≤30d 的「登录页 P50/P99 延迟」会变为空/零；**可用率（availability_pct）
  连续不受影响**。

**这是有意的诚实降级**：宁可丢失旧延迟分位，也不 resize 错位污染出虚假的 p99。

## 回滚

若需回滚到旧桶边界，反向操作同样会触发一次 series 断层与一次 availability_rollup 旧行重置。
