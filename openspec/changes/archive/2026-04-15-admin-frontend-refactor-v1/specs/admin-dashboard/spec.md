## MODIFIED Requirements

### Requirement: AdminDashboard uses StatCards and shows trend chart
`frontend/src/pages/admin/AdminDashboard.tsx` SHALL be modified:

**StatCard replacement:**
- Replace the 3 existing `Card` elements in the stats grid with `StatCard` components:
  - Users: `color="accent"`, icon = user group SVG path (from sidebarLinks "用户管理" icon), `trend` from `stats.trend?.users`
  - Words: `color="info"`, icon = book SVG path (from sidebarLinks "词书中心" icon), no trend (words are not time-dependent)
  - Records: `color="success"`, icon = chart SVG path (from sidebarLinks "数据分析" icon), `trend` from `stats.trend?.records`

**System status section upgrade:**
- Status field: prepend a colored dot — `bg-success` for "healthy", `bg-warning` for "degraded", `bg-error` for "down"
- Uptime: format as `Xd Xh Xm` (e.g. 86400 → "1d 0h 0m")
- Version + update link: unchanged behavior
- DB size: unchanged (already formatted as MB)

**New trend chart section:**
- Below system status, add a new `Card` with title "日活跃用户趋势"
- Call `adminApi.getDailyActiveUsers(7)` on mount (parallel with existing fetches via `Promise.allSettled`)
- Render ECharts line chart: X-axis = dates, Y-axis = user count, series color = `--accent`
- States:
  - Loading: `Skeleton` with `height="320px"`
  - API error / null data: `Skeleton` with `height="320px"` and no error message (silent degradation)
  - Empty array (all zeros): render chart normally showing flat zero line

**Remove page h1:** Delete `<h1 class="text-2xl font-bold text-content">仪表盘</h1>`

#### Scenario: Dashboard loads with all data
- **WHEN** stats, health, and daily-active-users all succeed
- **THEN** page shows 3 StatCards with trends, system status with StatusDot, and a line chart

#### Scenario: Trend API fails
- **WHEN** `getDailyActiveUsers` returns an error
- **THEN** chart area shows Skeleton placeholder; stats and system status render normally

#### Scenario: Stats include trend data
- **WHEN** `stats.trend.users.value` is 15
- **THEN** users StatCard shows "↑ 15% 较昨日" in success color
