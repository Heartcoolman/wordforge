## MODIFIED Requirements

### Requirement: MonitoringPage replaces all raw JSON with structured cards
`frontend/src/pages/admin/MonitoringPage.tsx` SHALL be completely refactored:

**System health card:**
- 4-cell grid layout (`grid-cols-2 md:grid-cols-4`):
  - Status: colored dot (success=green, warning=yellow, error=red based on `status` field) + text label ("运行正常"/"性能降级"/"服务异常")
  - Version: plain text `h().version`
  - Uptime: formatted as `Xd Xh Xm` from `uptimeSecs`
  - DB Size: `(dbSizeBytes / 1024 / 1024).toFixed(2) MB`
- Each cell: label as `text-xs text-content-tertiary` above, value as `text-sm font-medium text-content`
- Remove the `filterSensitiveFields` wrapper and `<pre>` block for this section

**Public health probe card:**
- Display `status` with colored Badge (success/error)
- Sub-services section: render each sub-service (store, amas, sse, wordbook-center) as a row with name + Badge (success when healthy, error when not)
- Note: backend needs to be extended to return sub-service statuses (see public-health-probe spec)
- If `publicHealth()` is null, show error card as before

**Database info card:**
- Display fields from the extended DatabaseInfo type:
  - Size: `(sizeOnDisk / 1024 / 1024).toFixed(2) MB`
  - Table Count: `tableCount`
  - Page Size: `pageSize` bytes
  - Page Count: `pageCount`
  - WAL Enabled: Badge success/warning ("启用"/"未启用")
- Layout: 2×3 grid of label-value pairs
- Remove `<pre>` block

**AMAS monitoring events card:**
- Title: "AMAS 监控事件"
- Each event rendered as a structured row within a scrollable container (`max-h-96 overflow-y-auto`):
  - Row header: timestamp (formatted via `toLocaleString()`) + `eventType` as colored Badge
  - Row body: generic recursive key-value renderer for `event.data`:
    - Primitive values: render as `key: value` in a flex row
    - Nested objects: render inside `<details><summary>key</summary>` with recursive content
    - Arrays: render as comma-separated values or nested details if elements are objects
- Remove `<pre>` block and `filterSensitiveFields` (data from admin API is already trusted)
- If `monitoring()` is null, show error card as before

**Remove page h1:** Delete `<h1>系统监控</h1>`

**Retain:** Error state handling for individual sections (partial failure shows error card for that section while others render normally).

#### Scenario: All monitoring data loads
- **WHEN** health, publicHealth, db, monitoring all succeed
- **THEN** page shows 4 structured cards with no raw JSON anywhere

#### Scenario: Health shows degraded status
- **WHEN** `health().status === "degraded"`
- **THEN** status dot is yellow/warning, label shows "性能降级"

#### Scenario: AMAS event has nested data
- **WHEN** an event's `data` contains `{ "userState": { "attention": 0.8, "fatigue": 0.3 }, "latencyMs": 12 }`
- **THEN** "latencyMs: 12" renders inline; "userState" renders as collapsible details containing "attention: 0.8" and "fatigue: 0.3"

#### Scenario: Database shows WAL enabled
- **WHEN** `db().walEnabled === true`
- **THEN** WAL field shows success Badge with "启用"
