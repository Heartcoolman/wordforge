## ADDED Requirements

### Requirement: Client periodically reports telemetry as delta increments
The authenticated client SHALL call `POST /api/telemetry` every 5 minutes with a delta-increment payload representing only the activity in that interval (not cumulative session totals). The telemetry worker is activated only when the user is authenticated. The endpoint is EXEMPT from the maintenance mode 503 middleware. The request body MUST NOT exceed 64 KB. `POST /api/telemetry` requires `AuthUser` JWT AND `X-Device-Id` header; missing `X-Device-Id` returns HTTP 400.

Request body schema:
```json
{
  "eventType": "periodic" | "on_demand",
  "requestId": "<uuid> | null",
  "clientTs": "<ISO8601 UTC>",
  "payload": {
    "sessionDurationSecs": <integer ≥ 0>,
    "actionsPerMin": <float ≥ 0>,
    "featureUsage": { "<feature>": <integer ≥ 0> },
    "errorCount": <integer ≥ 0>,
    "avgResponseTimeMs": <float ≥ 0>
  }
}
```

`requestId` MUST be present and non-null when `eventType = "on_demand"`; it is stored in `telemetry_events.triggered_by_request_id`. `clientTs` is stored as-is; all admin queries and sorting use `server_ts`.

#### Scenario: Periodic telemetry upload
- **WHEN** 5 minutes have elapsed since the last upload AND the user is authenticated
- **THEN** the client sends `POST /api/telemetry` with `eventType: "periodic"`, `requestId: null`, delta payload for that 5-minute window
- **THEN** the server responds HTTP 200 and stores the record with `server_ts = now`, `event_type = "periodic"`

#### Scenario: Missing X-Device-Id
- **WHEN** `POST /api/telemetry` is called without `X-Device-Id` header
- **THEN** the server returns HTTP 400 with `{ "code": "MISSING_DEVICE_ID" }`

#### Scenario: Payload exceeds 64 KB
- **WHEN** `POST /api/telemetry` body exceeds 64 KB
- **THEN** the server returns HTTP 413

#### Scenario: Unauthenticated client does not report
- **WHEN** the user is not logged in
- **THEN** the telemetry worker does not call `POST /api/telemetry`

#### Scenario: Telemetry accepted during maintenance
- **WHEN** maintenance mode is active AND an authenticated client calls `POST /api/telemetry`
- **THEN** the server accepts and stores the record (maintenance exemption applies)

#### Scenario: PBT — Payload values are non-negative integers/floats
- **WHEN** client submits `sessionDurationSecs < 0` or `errorCount < 0`
- **THEN** the server returns HTTP 422 (invalid payload)

#### Scenario: PBT — server_ts is authoritative for ordering
- **WHEN** client submits a `clientTs` with any clock skew (past or future)
- **THEN** `server_ts` is set to the server's `now` and is used for all ordering/display; `clientTs` is stored verbatim but not used for ordering

---

### Requirement: Admin can trigger on-demand telemetry pull
The system SHALL allow admin to trigger an immediate telemetry report from a specific device via `POST /api/admin/clients/:id/request-telemetry`. The server sends a `telemetry_request` SSE event to ALL active SSE connections of that device. The client MUST echo back the `requestId` in the subsequent `POST /api/telemetry` call. The admin API returns immediately after SSE delivery (fire-and-forget); it does NOT wait for the client's telemetry POST. If the device has no active SSE connection, the server returns HTTP 422.

#### Scenario: Device has active SSE connection
- **WHEN** admin calls `POST /api/admin/clients/:id/request-telemetry` AND the device has ≥1 active SSE connection
- **THEN** the server sends `{ "type": "telemetry_request", "requestId": "<uuid>" }` to ALL of the device's SSE connections
- **THEN** the server responds HTTP 200 immediately (does not wait for client upload)

#### Scenario: Client responds to telemetry_request
- **WHEN** client receives `{ "type": "telemetry_request", "requestId": "R" }` via SSE
- **THEN** the client immediately calls `POST /api/telemetry` with `eventType: "on_demand"`, `requestId: "R"`, and current delta payload

#### Scenario: Device has no active SSE connection
- **WHEN** admin calls `POST /api/admin/clients/:id/request-telemetry` AND no SSE connection exists for that device
- **THEN** the server returns HTTP 422 with `{ "code": "DEVICE_OFFLINE", "message": "设备当前无活跃 SSE 连接" }`

#### Scenario: PBT — requestId uniqueness
- **WHEN** admin triggers N on-demand pulls (N ≥ 2)
- **THEN** each `requestId` generated is a distinct UUID; no two pulls share the same `requestId`

---

### Requirement: Admin can view paginated telemetry history per device
The system SHALL provide `GET /api/admin/telemetry/:device_id?limit=N&offset=M` returning telemetry records for the specified device ordered by `server_ts DESC`. Default `limit = 50`, max `limit = 200`. Unknown `device_id` returns HTTP 404. Records include: `id`, `deviceId`, `userId`, `eventType`, `triggeredByRequestId`, `payload`, `clientTs`, `serverTs`.

#### Scenario: Device has telemetry records
- **WHEN** admin calls `GET /api/admin/telemetry/:device_id`
- **THEN** response contains records ordered by `server_ts DESC`, respecting `limit` and `offset` parameters

#### Scenario: Pagination
- **WHEN** admin calls `GET /api/admin/telemetry/:device_id?limit=10&offset=20`
- **THEN** response skips the 20 most recent records and returns the next 10

#### Scenario: Device has no telemetry records
- **WHEN** admin calls `GET /api/admin/telemetry/:device_id` for a known device with no records
- **THEN** response is `{ "ok": true, "data": { "records": [], "total": 0 } }`

#### Scenario: Unknown device_id
- **WHEN** admin calls `GET /api/admin/telemetry/:device_id` for a `device_id` not in `client_devices`
- **THEN** the server returns HTTP 404

#### Scenario: PBT — Pagination stability
- **WHEN** no new telemetry is written between two paginated queries with offset=0 and offset=N
- **THEN** the union of both result sets has no duplicates and covers exactly `N + page_size` distinct records (assuming sufficient total records)

#### Scenario: PBT — limit boundary enforcement
- **WHEN** admin calls with `limit > 200`
- **THEN** the server clamps to 200 and returns at most 200 records (no error)
