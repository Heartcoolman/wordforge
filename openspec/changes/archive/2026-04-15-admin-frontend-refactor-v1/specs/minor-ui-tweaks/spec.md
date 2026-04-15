## MODIFIED Requirements

### Requirement: ClientsPage telemetry panel visual enhancement
`frontend/src/pages/admin/ClientsPage.tsx` SHALL apply minor visual improvements:

- Telemetry summary groups ("设备信息", "会话统计", "行为摘要", "功能使用") each get an icon prefix:
  - 设备信息: device SVG icon, `text-info`
  - 会话统计: clock SVG icon, `text-accent`
  - 行为摘要: cursor SVG icon, `text-success`
  - 功能使用: puzzle SVG icon, `text-warning`
- Group title: add the icon (16×16 inline SVG) before the existing `font-semibold text-content-secondary` text, change text color to match the group's semantic color
- Table rows (both SSE and recent tabs): add `hover:bg-surface-secondary/50 transition-colors` to `<tr>` elements

**Remove page h1:** Delete `<h2 class="text-xl font-bold text-content">客户端管理</h2>` (note: this page uses h2 not h1)

#### Scenario: Telemetry panel shows group icons
- **WHEN** telemetry data is expanded for a device
- **THEN** each group header has a colored icon prefix matching its semantic category

#### Scenario: Table row hover
- **WHEN** user hovers over a table row in SSE or recent tab
- **THEN** row background transitions to `surface-secondary` at 50% opacity

---

### Requirement: UserManagementPage table styling consistency
`frontend/src/pages/admin/UserManagementPage.tsx` SHALL apply:

- Table `<tr>` elements in tbody: add `hover:bg-surface-secondary/40 transition-colors`
- Action column: change `space-x-2` to `gap-2` on the container (change `<td class="... space-x-2">` to flex container with `gap-2`)

**Remove page h1:** Delete `<h1 class="text-2xl font-bold text-content">用户管理</h1>`

#### Scenario: User table row hover
- **WHEN** user hovers over a user row
- **THEN** row background transitions to surface-secondary at 40% opacity

#### Scenario: Action buttons spacing
- **WHEN** action buttons are rendered
- **THEN** they use flex gap-2 instead of space-x-2

---

### Requirement: AdminLoginPage logo area
`frontend/src/pages/admin/AdminLoginPage.tsx` SHALL replace the text title with a logo area:

- Replace `<h1 class="text-2xl font-bold text-center text-content mb-6">管理后台登录</h1>` with:
  - A centered 48×48 rounded-full div with `bg-accent` containing a 24×24 white lock/shield SVG icon
  - Below it: "WordForge Admin" text as `text-lg font-semibold text-content`
  - Below it: `text-sm text-content-tertiary` "管理后台登录"
  - Wrapper: `flex flex-col items-center gap-2 mb-6`

#### Scenario: Login page renders logo
- **WHEN** admin login page loads
- **THEN** shows accent-colored circle icon + "WordForge Admin" + "管理后台登录" subtitle instead of plain text title
