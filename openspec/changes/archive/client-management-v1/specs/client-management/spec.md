## ADDED Requirements

### Requirement: Device identity registration via request headers
The system SHALL automatically register or update a device record in `client_devices` upon receiving an authenticated request carrying `X-Device-Id` and `X-Device-Platform` headers. `X-Device-Id` MUST be a valid UUID (v4, any case). `X-Device-Platform` MUST be one of: `web`, `ios`, `macos`, `ipados`; invalid values are stored as-is (no rejection). The `user_id` field is updated to the current JWT's user on every upsert, reflecting device ownership transfer when a different account uses the same device.

#### Scenario: First-time device seen
- **WHEN** an authenticated request carries a previously unseen `X-Device-Id`
- **THEN** the system inserts a new row in `client_devices` with `first_seen_at = now`, `last_seen_at = now`, `user_id` from JWT, `is_banned = 0`

#### Scenario: Returning device updates last_seen_at
- **WHEN** an authenticated request carries an existing `X-Device-Id`
- **THEN** the system updates `last_seen_at = now` and `user_id` from JWT; `first_seen_at` and ban fields remain unchanged

#### Scenario: Request without device headers
- **WHEN** a request is missing `X-Device-Id`
- **THEN** the request is processed normally; no device record is created or updated

#### Scenario: PBT — first_seen_at is immutable after creation
- **WHEN** a device sends N requests (N ≥ 2) over time
- **THEN** `first_seen_at` remains equal to the timestamp of the first request; only `last_seen_at` advances

#### Scenario: PBT — last_seen_at is monotonically non-decreasing
- **WHEN** a device sends requests at times T1 < T2 < ... < Tn
- **THEN** `last_seen_at` after each request Ti is ≥ `last_seen_at` before that request

---

### Requirement: Banned device is rejected at middleware level
The system SHALL reject all requests from a banned device (where `is_banned = 1` in `client_devices`) regardless of JWT validity. The device middleware runs AFTER authentication middleware but BEFORE route handlers. Device bans do NOT apply to requests missing the `X-Device-Id` header.

#### Scenario: Banned device sends authenticated request
- **WHEN** a request carries a `X-Device-Id` whose `is_banned = 1`
- **THEN** the system returns HTTP 403 with `{ "code": "CLIENT_BANNED", "message": "设备已被封禁" }` before any route handler executes

#### Scenario: Banned device isolation from user ban
- **WHEN** device D is banned but user U (who used D) is not banned
- **THEN** user U can access the system from other devices with different `X-Device-Id` values

#### Scenario: User ban does not affect device ban state
- **WHEN** user U is banned
- **THEN** `is_banned` in `client_devices` for any device used by U remains unchanged

#### Scenario: PBT — Ban is enforced on every subsequent request (not just the first)
- **WHEN** a device is banned at time T AND sends N requests after T (N ≥ 1)
- **THEN** every one of the N requests returns HTTP 403; the ban does not expire automatically

---

### Requirement: Active SSE connections tracked with multi-connection support
The system SHALL track active SSE connections in `AppState.active_sse: Arc<DashMap<String, Vec<SseClientInfo>>>`, allowing multiple simultaneous connections per `device_id`. A connection is added on establishment and removed immediately on disconnect (via Drop guard). `SseClientInfo` contains: `user_id`, `platform`, `connected_at`, and a `Sender` for per-connection message delivery.

#### Scenario: Client establishes SSE connection with device headers
- **WHEN** a client connects to the SSE endpoint with `X-Device-Id` present
- **THEN** a `SseClientInfo` entry is appended to the `Vec` for that `device_id` in `active_sse`

#### Scenario: Client disconnects SSE
- **WHEN** an SSE connection is closed (client disconnect or server drop)
- **THEN** the corresponding `SseClientInfo` entry is removed from the `Vec` for that `device_id`; if the `Vec` becomes empty, the key is removed from the map

#### Scenario: Same device with multiple tabs
- **WHEN** device D opens N simultaneous SSE connections (N ≥ 2)
- **THEN** `active_sse[device_id]` contains N entries; messages broadcast to that device are delivered to all N connections

#### Scenario: PBT — active_sse cardinality matches open connections
- **WHEN** a device opens C connections and closes K of them (K ≤ C)
- **THEN** `active_sse[device_id].len()` equals `C - K` at all times

---

### Requirement: Admin can view online clients
The system SHALL provide `GET /api/admin/clients` returning two lists: SSE live connections and recently active clients. "Recently active" means `last_seen_at >= now - 15 minutes`. A device MAY appear in both lists simultaneously (no deduplication). The `sseLive` list de-duplicates by `device_id` (showing one entry per device regardless of connection count, with `connectionCount` field). Response is not paginated.

#### Scenario: Active SSE connections present
- **WHEN** admin calls `GET /api/admin/clients`
- **THEN** response `sseLive` array contains one entry per unique `device_id` in `active_sse`, with fields: `deviceId`, `platform`, `userId`, `connectedSecs`, `connectionCount`

#### Scenario: Recently active non-SSE clients present
- **WHEN** admin calls `GET /api/admin/clients` and some devices have `last_seen_at >= now - 15 minutes`
- **THEN** response `recentlyActive` array contains those devices with fields: `deviceId`, `platform`, `userId`, `lastSeenAt`, `isBanned`

#### Scenario: Device in both lists
- **WHEN** a device has an active SSE connection AND `last_seen_at` is within 15 minutes
- **THEN** the device appears in BOTH `sseLive` and `recentlyActive`

#### Scenario: No active clients
- **WHEN** no SSE connections are open and no devices active in last 15 minutes
- **THEN** response is `{ "ok": true, "data": { "sseLive": [], "recentlyActive": [] } }`

---

### Requirement: Admin can ban and unban a client device
The system SHALL allow admin to ban via `POST /api/admin/clients/:id/ban` (body: `{ "reason": "<string>" }`, reason optional, max 500 chars) and unban via `POST /api/admin/clients/:id/unban`. Banning MUST immediately drop all active SSE connections for that `device_id`. Banning a non-existent `device_id` returns 404.

#### Scenario: Ban an existing device
- **WHEN** admin calls `POST /api/admin/clients/:id/ban`
- **THEN** `client_devices.is_banned = 1`, `banned_at = now`, `banned_by = admin user_id`, `ban_reason = reason`; any active SSE connections for that device are immediately dropped; subsequent requests from the device return 403

#### Scenario: Unban a device
- **WHEN** admin calls `POST /api/admin/clients/:id/unban`
- **THEN** `is_banned = 0`, `banned_at`, `banned_by`, `ban_reason` are set to NULL; subsequent requests from the device are processed normally

#### Scenario: Ban non-existent device
- **WHEN** admin calls `POST /api/admin/clients/:id/ban` for an unknown `device_id`
- **THEN** the system returns HTTP 404

#### Scenario: PBT — Ban/unban is idempotent
- **WHEN** admin bans an already-banned device (or unbans an already-unbanned device)
- **THEN** the operation succeeds (HTTP 200) and the state remains consistent; no error is returned
