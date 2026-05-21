# rc-observation — rc.3 稳态观测脚手架

M2-Q4 任务产物。rc.3 发布后连续 7 天三源全绿才可触发 GA。

## 目录结构（设计草稿，待 Q1/Q2/Q3 完成后实施）

```
scripts/rc-observation/
├── README.md                   # 本文件
├── collect_5xx_rate.sh         # 源①：读 /metrics 的 http_5xx / http_requests → 计算滚动错误率
├── collect_sse_incidents.sh    # 源②：读 admin SSE 告警流 → 记录 incident 事件到 incident.log
├── collect_gh_regressions.sh   # 源③：GitHub issue tracker → 统计 regression label 新增数
└── daily_report.sh             # 汇总三源 → 输出 JSON 日报 + 判定当日是否全绿
```

关联产物（待 Q1/Q2/Q3 后同步实施）：

```
docs/runbook/
├── rc-observation-report.md        # 仪表板 / HTML+MD 报告模板
└── rc-observation-thresholds.md    # 告警阈值定义

tests/
└── rc_observation_scripts.rs       # 集成测试：验证脚本可跑
```

## 三数据源说明

| 源 | 脚本 | 核心指标 | 阈值 |
|----|------|---------|------|
| `/metrics` | `collect_5xx_rate.sh` | `http_5xx / http_requests` 滚动 1h | > 0.1% 告警 |
| admin SSE | `collect_sse_incidents.sh` | `incident` 类型事件累计数 | 新增任意 1 条告警 |
| GitHub issues | `collect_gh_regressions.sh` | `regression` label 新增 issue 数 | 新增任意 1 条告警 |

## 使用方式（实施后）

```bash
# 单次采集（cron 每小时执行）
./scripts/rc-observation/collect_5xx_rate.sh \
  --url "https://your-domain" \
  --token "$ADMIN_TOKEN" \
  --output /tmp/rc-obs/metrics.json

# 每日汇总（cron 23:59 执行）
./scripts/rc-observation/daily_report.sh \
  --day "2026-05-28" \
  --obs-dir /tmp/rc-obs \
  --output docs/runbook/rc-observation-report.md

# 连续 7 天判定
jq '[.[] | .all_green] | all' /tmp/rc-obs/day-*.json
```

## 前置条件

- M2-Q1 完成：`/metrics` 暴露 histogram `http_request_duration_seconds`
- M2-Q2 完成：Lighthouse CI 通过
- M2-Q3 完成：0 P0 / 0 P1 契约对齐
- 环境变量：`WORDFORGE_URL`、`ADMIN_TOKEN`、`GH_TOKEN`（read:issues scope）

## GA 判定逻辑

7 个日报 JSON 中 `all_green: true` 全部满足，才可在 GitHub Releases 发布不带 `-` 的 `v1.0.0` tag。

任一天不达标：停 GA，修复，从第 1 天重新计数。
