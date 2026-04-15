## MODIFIED Requirements

### Requirement: AmasConfigPage metrics displayed as table instead of raw JSON
`frontend/src/pages/admin/AmasConfigPage.tsx` SHALL modify the metrics display section:

**Replace `<pre>` with structured table:**
- Table columns: 算法名称 | 调用次数 | 平均延迟 | 错误次数
- Each row from `Object.entries(metrics())`:
  - 算法名称: key string, `font-mono text-sm`
  - 调用次数: `snapshot.callCount`, right-aligned
  - 平均延迟: `(snapshot.totalLatencyUs / snapshot.callCount / 1000).toFixed(2) ms` (if callCount > 0, else "–"), right-aligned
  - 错误次数: `snapshot.errorCount`, right-aligned; if > 0, render in `text-error`
- Table uses existing project table styling conventions: `text-sm`, header row `bg-surface-secondary border-b border-border`, data rows with `border-b border-border/50`
- Table row hover: `hover:bg-surface-secondary/50 transition-colors`
- If `metrics()` is empty object: show "暂无指标数据" text

**Config editor textarea: unchanged.**

**Remove page h1:** Delete `<h1>AMAS 配置</h1>`

#### Scenario: Metrics table renders
- **WHEN** `metrics()` returns `{ "heuristic": { callCount: 100, totalLatencyUs: 500000, errorCount: 0 }, "ige": { callCount: 50, totalLatencyUs: 300000, errorCount: 2 } }`
- **THEN** table shows 2 rows: "heuristic | 100 | 5.00 ms | 0" and "ige | 50 | 6.00 ms | 2" (2 in red)

#### Scenario: Algorithm with zero calls
- **WHEN** `callCount` is 0
- **THEN** average latency column shows "–" instead of NaN

#### Scenario: No metrics available
- **WHEN** `metrics()` is `{}`
- **THEN** shows "暂无指标数据" text instead of empty table
