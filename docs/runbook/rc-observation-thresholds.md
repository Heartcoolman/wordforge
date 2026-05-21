# rc.3 稳态观测告警阈值（M2-Q4）

rc.3 发布后连续 **7 天三源全绿**才可触发 GA。任一天任一源不满足 → 停 GA，修复，从第 1 天重新计数。

---

## 三数据源阈值定义

### 源① `/metrics` 5xx 错误率

| 参数 | 值 | 说明 |
|---|---|---|
| 采集端点 | `GET /metrics` | Admin JWT 鉴权 |
| 采集频率 | 每 5 分钟一次（cron `*/5 * * * *`） |  |
| 计算方式 | `http_5xx / http_requests`（快照差值，滚动窗口） | 同后端 `error_rate_watchdog` |
| **告警阈值** | **> 0.1%**（0.001） | RFC §6.4 GA 门定义 |
| 去重窗口 | 5 分钟内同一 incident 不重复计入 | 与后端 `DEDUP_SECS=300` 对齐 |
| 日判定逻辑 | 当日所有采集点均 ≤ 0.1% → `5xx_green: true` | 任一点超限 → 当日不绿 |

### 源② Admin SSE Incident 事件

| 参数 | 值 | 说明 |
|---|---|---|
| 采集方式 | 轮询 `/metrics` 差值（等效于 SSE incident 触发条件） | SSE 长连接不适合 cron，改用差值检测 |
| 采集频率 | 每 5 分钟（与源① 共用一次 `/metrics` 请求） |  |
| **告警阈值** | **累计 incident 事件 ≥ 1 条** | 零容忍：任何 incident 均需处置后重启观测 |
| incident 定义 | 滚动 5 分钟 5xx/total > 1%（后端 `THRESHOLD=0.01`） | 源①阈值更严（0.1%），incident 是应急告警 |
| 日判定逻辑 | 当日 incident 计数 = 0 → `sse_green: true` |  |

### 源③ GitHub Issue Regression Label

| 参数 | 值 | 说明 |
|---|---|---|
| 采集端点 | `GET /repos/Heartcoolman/wordforge/issues?labels=regression&state=open&since=<rc3_date>` |  |
| 采集频率 | 每天一次（cron `0 8 * * *`） | GitHub API rate limit 友好 |
| **告警阈值** | **since rc.3 发布日起 regression 新增 ≥ 1 条** | 零容忍 |
| 所需权限 | `GH_TOKEN` with `read:issues` | public repo 可不带 token（rate limit 60/h） |
| 日判定逻辑 | regression_count = 0 → `gh_green: true` |  |

---

## GA 门禁触发条件（RFC §6.4）

```
7 个连续 day-YYYY-MM-DD.json 文件中：
  all_green == true (全部)
→ 可发布 v1.0.0 tag（不带 "-"）
```

**`all_green = 5xx_green AND sse_green AND gh_green`**

---

## 异常处置

| 异常 | 操作 |
|---|---|
| 源① 5xx 超阈值 | 查 `docs/runbook/incident-response.md` §5xx 上涨 → 修复 → 重置观测计数 |
| 源② incident 触发 | 同上；检查 `/admin` Dashboard incident badge |
| 源③ regression 新增 | 分析 issue 是否确属 rc.3 引入回归；确属则修 + reopen 观测 |
| 采集脚本自身失败 | 视为当日数据缺失，不计为绿（保守策略）；查 cron 日志 |
| 7 天窗口期间发新补丁 | 若补丁属于 P0 修复，重置计数；若仅文档/配置，由 team-lead 裁定是否重置 |

---

## 相关文件

- `scripts/rc-observation/collect_5xx_rate.sh`
- `scripts/rc-observation/collect_sse_incidents.sh`
- `scripts/rc-observation/collect_gh_regressions.sh`
- `scripts/rc-observation/daily_report.sh`
- `docs/runbook/rc-observation-report.md`（报告模板）
- `tests/rc_observation_scripts.rs`（集成测试）
