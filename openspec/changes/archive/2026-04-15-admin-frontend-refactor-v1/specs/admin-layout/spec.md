## MODIFIED Requirements

### Requirement: AdminLayout header shows dynamic page title and admin identity
`frontend/src/components/layout/AdminLayout.tsx` SHALL be modified:

**Header dynamic title:**
- Replace static "管理后台" text with the `label` from the matching `sidebarLinks` entry based on `useLocation().pathname`
- Match algorithm: find the link where `link.exact ? pathname === link.href : pathname.startsWith(link.href)` (same logic as `isActive`)
- Fallback for unmatched routes: display "管理后台"

**Header right section — admin email + logout:**
- On layout mount, call `adminApi.verifyToken()` once and cache result in a component-level signal
- Display admin email as `text-sm text-content-secondary`
- Loading state: show `Skeleton` with `width="120px"` while verifyToken is pending
- Failure state (401 or network error): show nothing (email area hidden); do NOT redirect — login page handles auth
- Logout button: icon-only button (same SVG as current sidebar logout) with `hover:text-error` style; onClick triggers same logout logic as current sidebar button

**Sidebar bottom logout removal:**
- Remove the entire `<div class="border-t ...">` section containing the logout button from the sidebar
- Sidebar now ends at the nav section

**Page-level h1 removal:**
- All admin page components (`AdminDashboard`, `AnalyticsPage`, `MonitoringPage`, `AmasConfigPage`, `ClientsPage`, `UserManagementPage`) SHALL have their `<h1>` element removed since the Header now provides the page title

#### Scenario: Navigate to dashboard
- **WHEN** pathname is `/admin`
- **THEN** header shows "仪表盘" (matched with `exact: true`)

#### Scenario: Navigate to monitoring page
- **WHEN** pathname is `/admin/monitoring`
- **THEN** header shows "系统监控"

#### Scenario: Unknown admin route
- **WHEN** pathname is `/admin/some-unknown-path`
- **THEN** header shows "管理后台" (fallback)

#### Scenario: verifyToken succeeds
- **WHEN** layout mounts and `verifyToken()` returns `{ id, email }`
- **THEN** header right shows the admin email and logout icon button

#### Scenario: verifyToken fails
- **WHEN** `verifyToken()` returns 401
- **THEN** header right shows only the logout icon button; email area is hidden

#### Scenario: Sidebar in collapsed state
- **WHEN** sidebar is collapsed
- **THEN** sidebar has no logout button at bottom; header still shows email and logout
