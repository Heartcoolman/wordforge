# rc.3 稳态观测报告模板（M2-Q4）

> 使用方式：`daily_report.sh` 每日生成 `day-YYYY-MM-DD.json`；本文件为人工汇总模板，
> 在 7 天观测结束后填写，附在 v1.0 GA PR 描述中作为质量门证据。

---

## 观测区间

| 字段 | 值 |
|---|---|
| rc.3 发布时间 | `<YYYY-MM-DDTHH:MM:SSZ>` |
| 观测开始 | Day 1：`<YYYY-MM-DD>` |
| 观测结束 | Day 7：`<YYYY-MM-DD>` |
| 观测版本 | `v1.0-rc.3` |
| 观测环境 | 生产（`8.135.57.148`，见 [[wordforge_prod_deployment]]） |

---

## 七天汇总

| 天 | 日期 | 5xx 错误率 | SSE Incident | GH Regression | all_green |
|---|---|---|---|---|---|
| Day 1 | `<date>` | `<rate>` | 0 | 0 | ✅ / ❌ |
| Day 2 | `<date>` | `<rate>` | 0 | 0 | ✅ / ❌ |
| Day 3 | `<date>` | `<rate>` | 0 | 0 | ✅ / ❌ |
| Day 4 | `<date>` | `<rate>` | 0 | 0 | ✅ / ❌ |
| Day 5 | `<date>` | `<rate>` | 0 | 0 | ✅ / ❌ |
| Day 6 | `<date>` | `<rate>` | 0 | 0 | ✅ / ❌ |
| Day 7 | `<date>` | `<rate>` | 0 | 0 | ✅ / ❌ |
| **7 天合计** | — | **max: `<rate>`** | **`<n>` 条** | **`<n>` 条** | **✅ / ❌** |

---

## 数据源说明

### 源① 5xx 错误率

阈值：> 0.1% 告警（RFC §6.4）

```bash
# 查看每日最高错误率
for f in /tmp/rc-obs/5xx_*.json; do
    python3 -c "import json; d=json.load(open('$f')); print(d['ts'], d['error_rate'])"
done
```

最高值：`<fill>`  
最低值：`<fill>`  
平均值：`<fill>`

### 源② Admin SSE Incident

```bash
# 查看 incident 日志
cat /tmp/rc-obs/incidents.log | python3 -m json.tool
```

Incident 总数：`<n>` 条  
详情：`<none / 列出每条>`

### 源③ GitHub Regression Issues

```bash
# 查看最终状态
cat /tmp/rc-obs/regressions_<last_day>.json | python3 -m json.tool
```

新增 regression issue：`<n>` 条  
详情：`<none / 列出每条>`

---

## GA 判定

```bash
# 自动判定 7 天全绿
jq '[.[] | .all_green] | all' /tmp/rc-obs/day-*.json
```

结果：`true / false`

**结论**：

- [ ] 7 天三源全绿 → 可发 `v1.0.0` tag
- [ ] 存在不绿天数 → 停 GA，需修复并重置观测

---

## 签署

| 角色 | 姓名 | 日期 |
|---|---|---|
| QA | `dev-qa-1` | `<date>` |
| Tech Lead | `team-lead-2` | `<date>` |

---

*模板来源：`docs/runbook/rc-observation-report.md`，由 M2-Q4 任务生成。*  
*告警阈值定义见 `docs/runbook/rc-observation-thresholds.md`。*
