## ADDED Requirements

### Requirement: UpdatePayload broadcast channel in AppState
The system SHALL define a standalone `UpdatePayload` struct with `version: String` and `message: String` fields (NOT a variant of the existing `SseEvent` enum). `AppState` SHALL contain `update_tx: broadcast::Sender<UpdatePayload>` initialized with capacity 16. The `broadcast_update(version: Option<&str>, message: Option<&str>)` method SHALL construct an `UpdatePayload` using `env!("CARGO_PKG_VERSION")` as the default version and `"有新版本可用，请刷新页面获取最新内容"` as the default message, then send it via `update_tx`.

#### Scenario: Broadcast with both fields provided
- **WHEN** `broadcast_update(Some("1.2.0"), Some("新词书已上线"))` is called
- **THEN** all subscribers receive `UpdatePayload { version: "1.2.0", message: "新词书已上线" }`

#### Scenario: Broadcast with defaults
- **WHEN** `broadcast_update(None, None)` is called
- **THEN** all subscribers receive `UpdatePayload { version: env!("CARGO_PKG_VERSION"), message: "有新版本可用，请刷新页面获取最新内容" }`

#### Scenario: PBT — version is never empty after broadcast
- **WHEN** `broadcast_update` is called with `version ∈ {None, Some(""), Some(" "), Some(random_utf8)}`
- **THEN** the resulting `UpdatePayload.version` is always non-empty: `None` maps to `CARGO_PKG_VERSION`, `Some("")` or whitespace-only SHOULD also fall back to `CARGO_PKG_VERSION`

#### Scenario: PBT — broadcast fan-out to all active SSE connections
- **WHEN** N ≥ 0 SSE connections are subscribed and a single `UpdatePayload p` is broadcast
- **THEN** every non-lagged subscriber receives exactly `p` once; no duplicate or partial delivery occurs

---

### Requirement: SSE handler listens on update_rx broadcast
The SSE handler in `src/routes/realtime.rs` SHALL add a new `tokio::select!` branch subscribing to `state.update_rx()`. On receiving an `UpdatePayload`, the handler SHALL serialize it to JSON and emit an SSE event with `event: update_available`. On `Lagged` error, the handler SHALL silently skip (polling serves as fallback). The `update_rx` receiver SHALL be obtained before the stream loop begins.

#### Scenario: SSE client receives update_available event
- **WHEN** admin triggers a broadcast while a client has an active SSE connection
- **THEN** the client receives `event: update_available\ndata: {"version":"...","message":"..."}\n\n` within 1 second

#### Scenario: SSE client reconnects after server restart
- **WHEN** the server restarts (new binary version) and a client reconnects SSE
- **THEN** the SSE handler does NOT auto-push an update event; version detection relies on `/api/status` polling

#### Scenario: PBT — UpdatePayload round-trip through SSE serialization
- **WHEN** an arbitrary valid `UpdatePayload p = { version, message }` is serialized via `serde_json::to_string` into SSE data, then parsed by the frontend `JSON.parse`
- **THEN** the deserialized object has identical `version` and `message` values

#### Scenario: PBT — channel capacity boundary with lagged receiver
- **WHEN** a receiver falls behind by more than the channel capacity (16) messages
- **THEN** the receiver encounters a `Lagged` error which is silently handled; no panic, no partial message, no corrupted payload; the receiver resumes with subsequent messages

---

### Requirement: Admin broadcast-update endpoint
The system SHALL provide `POST /api/admin/broadcast-update` protected by `AdminAuthUser`. The request body SHALL accept optional `version: String` and optional `message: String`. The endpoint SHALL call `state.broadcast_update(version, message)` and return `ok(json!({ "broadcasted": true }))` following the project's unified response wrapper. Zero active SSE subscribers is NOT an error — the endpoint still returns success.

#### Scenario: Admin broadcasts with custom message
- **WHEN** admin calls `POST /api/admin/broadcast-update` with body `{ "message": "新功能上线" }`
- **THEN** response is `{ "ok": true, "data": { "broadcasted": true } }` and all SSE clients receive the event

#### Scenario: Admin broadcasts with empty body
- **WHEN** admin calls `POST /api/admin/broadcast-update` with body `{}`
- **THEN** response is `{ "ok": true, "data": { "broadcasted": true } }` and payload uses default version and message

#### Scenario: No SSE subscribers
- **WHEN** admin calls `POST /api/admin/broadcast-update` with no active SSE connections
- **THEN** response is still `{ "ok": true, "data": { "broadcasted": true } }` (fire-and-forget semantics)

#### Scenario: Unauthorized access
- **WHEN** a non-admin user calls `POST /api/admin/broadcast-update`
- **THEN** the system returns HTTP 401

#### Scenario: PBT — message length boundary: complete delivery or reject
- **WHEN** admin sends a request with `message` of arbitrary length L ≥ 0 (including multi-byte Unicode, newlines, JSON special characters)
- **THEN** either the request is explicitly rejected, or all SSE clients receive the `message` byte-identical to the original; silent truncation is forbidden

---

### Requirement: Frontend shared updateInfo signal for SSE and polling
The frontend SHALL maintain a single reactive signal `updateInfo: Signal<{ version: string; message: string } | null>` exported from `client.ts`. Both SSE `update_available` events and `/api/status` polling version changes SHALL write to this same signal. The `UpdateBanner` component SHALL derive its visibility solely from `updateInfo() !== null`.

#### Scenario: SSE triggers update banner
- **WHEN** an SSE `update_available` event arrives with `{ version: "1.1.0", message: "更新" }`
- **THEN** `updateInfo()` becomes `{ version: "1.1.0", message: "更新" }` and `UpdateBanner` is displayed

#### Scenario: Polling detects version change
- **WHEN** the initial `/api/status` returns `version: "1.0.0"` and a subsequent poll returns `version: "1.1.0"`
- **THEN** `updateInfo()` becomes `{ version: "1.1.0", message: "有新版本可用，请刷新页面获取最新内容" }` (default message)

#### Scenario: SSE and polling fire simultaneously
- **WHEN** both SSE and polling detect an update at nearly the same time
- **THEN** `updateInfo()` reflects the last write (signal convergence); the banner displays exactly once

#### Scenario: PBT — shared signal source-agnostic
- **WHEN** an arbitrary interleaved sequence of `SseWrite(p)`, `PollWrite(p)`, and `Close` events is applied
- **THEN** `bannerVisible ⟺ updateInfo !== null` holds at every state; `reduce(s, SseWrite(p)) = reduce(s, PollWrite(p)) = Some(p)` for any payload `p`

---

### Requirement: UpdateBanner closeable component with re-appearance
The frontend SHALL provide `UpdateBanner.tsx` as a fixed-position top banner (z-index 40, info/blue theme). The banner SHALL display `updateInfo().message`, a "刷新" button (`window.location.reload()`), and a "关闭" button (`setUpdateInfo(null)`). Closing the banner does NOT suppress future events — the next SSE `update_available` event SHALL re-display the banner regardless of version. The banner SHALL be mounted at the top level of `App.tsx` inside `MaintenanceProvider`, ensuring persistence across SPA route changes. Maintenance mode overlay and update banner are independent and MAY coexist.

#### Scenario: User closes banner then receives new SSE event
- **WHEN** user closes the update banner, then a new SSE `update_available` event arrives (same or different version)
- **THEN** the banner re-appears with the new event's message

#### Scenario: User clicks refresh
- **WHEN** user clicks the "刷新" button on the update banner
- **THEN** `window.location.reload()` is called

#### Scenario: Banner persists across route navigation
- **WHEN** the update banner is visible and the user navigates to a different SPA route
- **THEN** the banner remains visible (mounted at top level, not per-route)

#### Scenario: Banner during maintenance mode
- **WHEN** maintenance mode is active and an update event arrives
- **THEN** both the maintenance overlay and the update banner are displayed independently

#### Scenario: PBT — close then re-trigger is never permanently suppressed
- **WHEN** a `Close` action is followed by an SSE `UpdatePayload q`
- **THEN** `reduce(Some(p), Close) = None` AND `reduce(None, SseWrite(q)) = Some(q)`; specifically `reduce(Some(p), Close · SseWrite(p)) = Some(p)` (same version re-appears)

---

### Requirement: Admin UI trigger button in SettingsPage
The admin frontend SHALL add an "更新通知" card below the existing broadcast section in `SettingsPage.tsx`. The card SHALL contain an optional message input (placeholder: default message text) and a "发送更新通知" button with a confirmation dialog matching the existing broadcast confirmation pattern. The button SHALL call `adminApi.broadcastUpdate({ message })`.

#### Scenario: Admin sends update notification via UI
- **WHEN** admin enters a custom message and confirms the broadcast
- **THEN** `POST /api/admin/broadcast-update` is called with `{ "message": "<custom>" }` and a success toast is shown

#### Scenario: Admin sends with empty message
- **WHEN** admin clicks send without entering a message
- **THEN** `POST /api/admin/broadcast-update` is called with `{}` (server uses default message)
