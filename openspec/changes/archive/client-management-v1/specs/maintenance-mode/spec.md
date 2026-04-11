## ADDED Requirements

### Requirement: Status endpoint reports maintenance state
The system SHALL expose a public `GET /api/status` endpoint (no authentication required) that returns the current maintenance mode state and server version, wrapped in the standard `ok()` response envelope. The `version` field SHALL be sourced from `env!("CARGO_PKG_VERSION")`. The maintenance state SHALL be read from an `AtomicBool` cached in `AppState`, initialized synchronously from the database before the HTTP server accepts any traffic.

#### Scenario: Normal operation
- **WHEN** maintenance mode is disabled
- **THEN** `GET /api/status` returns HTTP 200 with `{ "ok": true, "data": { "maintenanceMode": false, "version": "<semver>" } }`

#### Scenario: Maintenance active
- **WHEN** maintenance mode is enabled
- **THEN** `GET /api/status` returns HTTP 200 with `{ "ok": true, "data": { "maintenanceMode": true, "version": "<semver>" } }`

#### Scenario: PBT — Status endpoint always returns 200 regardless of maintenance state
- **WHEN** maintenance mode is toggled to any value (true or false)
- **THEN** `GET /api/status` ALWAYS returns HTTP 200 (never 503); the `maintenanceMode` field reflects the current state

#### Scenario: PBT — AtomicBool reflects DB state after startup
- **WHEN** the application starts with `maintenance_mode = true` in the database
- **THEN** the first request handled returns the correct `maintenanceMode: true` (no window where stale false is returned)

---

### Requirement: Maintenance middleware blocks non-admin traffic
The system SHALL intercept all requests to paths NOT matching `/api/admin/**`, `/api/status`, `/api/realtime/**`, `/api/telemetry`, or `/health` when maintenance mode is active, and return 503. The exemption list is exhaustive; all other paths are blocked.

#### Scenario: Non-admin request during maintenance
- **WHEN** maintenance mode is enabled AND a request arrives at any non-exempt path
- **THEN** the system returns HTTP 503 with `{ "code": "MAINTENANCE", "message": "服务器维护中，请稍后重试" }`

#### Scenario: Admin request during maintenance
- **WHEN** maintenance mode is enabled AND a request arrives at `/api/admin/**`
- **THEN** the request is processed normally (not blocked)

#### Scenario: Status check during maintenance
- **WHEN** maintenance mode is enabled AND a request arrives at `GET /api/status`
- **THEN** the request is processed normally (not blocked)

#### Scenario: SSE connection during maintenance
- **WHEN** maintenance mode is enabled AND a client connects to `/api/realtime/**`
- **THEN** the connection is accepted normally (not blocked), allowing the client to receive the maintenance-ended event

#### Scenario: Telemetry upload during maintenance
- **WHEN** maintenance mode is enabled AND a client calls `POST /api/telemetry`
- **THEN** the request is processed normally (not blocked)

#### Scenario: PBT — Idempotency of 503 response
- **WHEN** maintenance is active AND the same non-exempt request is made N times (N ≥ 1)
- **THEN** every request returns HTTP 503 with identical `code` and `message`; no state change occurs in the database

#### Scenario: PBT — Maintenance state is monotonically consistent
- **WHEN** admin sets maintenance_mode = true at time T
- **THEN** ALL subsequent non-exempt requests (T+δ for any δ > 0) return 503 until admin sets maintenance_mode = false; no interleaving of 200 and 503 occurs for the same path

---

### Requirement: SSE maintenance event broadcast
The system SHALL broadcast a `maintenance` SSE event to all active SSE connections within 5 seconds of an admin toggling maintenance mode via `PUT /api/admin/settings`. The broadcast uses an existing `tokio::sync::broadcast::Sender<bool>` stored in `AppState.maintenance_tx`.

#### Scenario: Maintenance enabled via admin API
- **WHEN** admin calls `PUT /api/admin/settings` with `maintenance_mode: true`
- **THEN** all active SSE clients receive the SSE event `{ "type": "maintenance", "active": true }` within 5 seconds

#### Scenario: Maintenance disabled via admin API
- **WHEN** admin calls `PUT /api/admin/settings` with `maintenance_mode: false`
- **THEN** all active SSE clients receive the SSE event `{ "type": "maintenance", "active": false }` within 5 seconds

#### Scenario: PBT — All connected clients receive the event (broadcast coverage)
- **WHEN** N SSE connections are active (N ≥ 1) AND maintenance is toggled
- **THEN** all N connections receive the event; no connection is skipped

#### Scenario: PBT — No duplicate events on single toggle
- **WHEN** admin sets maintenance_mode = true exactly once
- **THEN** each SSE client receives exactly one `maintenance` event (not 2+)

---

### Requirement: Frontend maintenance lock
The client SHALL lock all navigation to the maintenance screen upon receiving a maintenance signal, and automatically restore normal operation when maintenance ends, without requiring a page reload. The admin SPA routes are NOT exempt—all frontend routes lock during maintenance.

#### Scenario: SSE maintenance event received
- **WHEN** the client receives `{ "type": "maintenance", "active": true }` via SSE
- **THEN** the frontend renders `MaintenancePage` replacing all routes and disables navigation

#### Scenario: Polling detects maintenance on startup or during session
- **WHEN** `GET /api/status` returns `maintenanceMode: true` (polled every 30 seconds, and on startup)
- **THEN** the frontend renders `MaintenancePage` replacing all routes and disables navigation

#### Scenario: Maintenance ends — SSE signal
- **WHEN** the client receives `{ "type": "maintenance", "active": false }` via SSE
- **THEN** the frontend automatically restores the previous route without a page reload

#### Scenario: Maintenance ends — polling fallback
- **WHEN** SSE is unavailable AND `GET /api/status` returns `maintenanceMode: false`
- **THEN** the frontend automatically restores normal routing on the next 30-second poll cycle

#### Scenario: PBT — Maintenance state is idempotent on repeated signals
- **WHEN** the client receives `{ "type": "maintenance", "active": true }` multiple times in sequence
- **THEN** the UI remains in maintenance lock state; no visual flash or route change occurs
