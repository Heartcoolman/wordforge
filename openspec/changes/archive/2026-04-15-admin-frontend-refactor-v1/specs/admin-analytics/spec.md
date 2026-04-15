## MODIFIED Requirements

### Requirement: AnalyticsPage uses StatCards and shows trend chart
`frontend/src/pages/admin/AnalyticsPage.tsx` SHALL be modified:

**User engagement section:**
- Replace 3 text blocks with `StatCard` components:
  - Total Users: `color="accent"`, user icon, no trend
  - Active Today: `color="success"`, activity icon, `trend` from `engagement.trend?.activeToday`
  - Retention Rate: `color="info"`, percentage icon, no trend
- `retentionRate` value formatted via `formatPercent()`

**Learning data section:**
- Replace 4 text blocks with `StatCard` components:
  - Total Words: `color="accent"`, book icon, no trend
  - Total Records: `color="info"`, list icon, `trend` from `learning.trend?.totalRecords`
  - Total Correct: `color="success"`, check icon, no trend
  - Overall Accuracy: `color="warning"`, target icon, `trend` from `learning.trend?.overallAccuracy`

**New chart section:**
- Below learning data, add a `Card` with title "每日学习记录"
- Call `adminApi.getDailyRecords(7)` on mount (parallel with existing fetches)
- Render ECharts bar chart:
  - X-axis: dates
  - Two series side-by-side: "正确" (color: `--success`) and "错误" (color: `--error`)
  - "错误" derived as `total - correct` from API data
  - Tooltip shows: date, correct count, incorrect count, total, accuracy percentage
  - Legend: "正确", "错误"
- States:
  - Loading: `Skeleton` with `height="320px"`
  - API error / null: `Skeleton` placeholder (silent degradation)
  - All zeros: render chart with empty bars

**Remove page h1:** Delete `<h1 class="text-2xl font-bold text-content">数据分析</h1>`

#### Scenario: Analytics loads fully
- **WHEN** engagement, learning, and daily-records all succeed
- **THEN** page shows 7 StatCards and a bar chart with correct/incorrect series

#### Scenario: Chart data has no records on a day
- **WHEN** 2026-04-11 has `correct: 0, total: 0`
- **THEN** that date shows two zero-height bars

#### Scenario: Learning trend shows decrease
- **WHEN** `learning.trend.totalRecords.value` is -8
- **THEN** Total Records StatCard shows "↓ 8% 较昨日" in error color
