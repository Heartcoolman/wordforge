## ADDED Requirements

### Requirement: Server-side heartbeat watchdog monitors active SSE devices
The server SHALL maintain a background Tokio task (`heartbeat_watchdog`) that runs in a loop with a 5-second tick. On each tick, it scans all devices in `AppState.active_sse` (i.e., devices with at least one live SSE connection) and checks whether a `periodic` or `session_start` telemetry record was received within the last 5 seconds.

The watchdog uses two new fields in `AppState`:
- `last_heartbeat: Arc<DashMap<String, Instant>>` — updated by `src/routes/telemetry.rs` on every successful telemetry insert
- `heartbeat_miss_count: Arc<DashMap<String, u8>>` — incremented per tick when a device misses its heartbeat; reset to 0 on receipt

When a device accumulates **5 consecutive misses** (≥25 seconds of silence), the watchdog:
1. Sends a `{ "type": "data_corrupted" }` SSE event to ALL active SSE connections of that device
2. Resets `heartbeat_miss_count[device_id]` to 0 (prevents repeated firing until next miss cycle)

The watchdog task is spawned in `main.rs` using `tokio::spawn`. It receives a clone of `AppState` and a clone of the shutdown broadcast receiver; it exits gracefully on shutdown signal.

#### SseEvent extension
`src/state.rs` `SseEvent` enum gains a new variant:
```rust
#[serde(rename = "data_corrupted")]
DataCorrupted,
```

#### AppState extension
`src/state.rs` `AppState` gains two new fields:
```rust
last_heartbeat: Arc<DashMap<String, Instant>>,
heartbeat_miss_count: Arc<DashMap<String, u8>>,
```
Both are initialized as empty `DashMap`s in `AppState::new()`.

When a new SSE connection is added to `active_sse`, the SSE handler SHALL immediately insert `last_heartbeat.insert(device_id.to_string(), Instant::now())` to prevent cold-start miss accumulation.

`AppState` exposes accessor methods `last_heartbeat()` and `heartbeat_miss_count()`.

#### Telemetry route update
`src/routes/telemetry.rs` `submit_telemetry` handler, after a successful `insert_telemetry()` call, SHALL:
```rust
state.last_heartbeat().insert(device_id.to_string(), Instant::now());
state.heartbeat_miss_count().insert(device_id.to_string(), 0);
```

#### Watchdog pseudocode
```
every 5s:
  for device_id in active_sse.keys():
    last = last_heartbeat.get(device_id) ?? Instant::now()  // new connection: grace period
    elapsed = last.elapsed()
    if elapsed > 10s:  // jitter buffer: one full client interval as slack
      count = heartbeat_miss_count.get(device_id) ?? 0 + 1
      heartbeat_miss_count.insert(device_id, count)
      if count >= 5:
        send DataCorrupted SSE to all connections of device_id
        heartbeat_miss_count.insert(device_id, 0)
    else:
      heartbeat_miss_count.insert(device_id, 0)
```

#### Scenario: Device sends heartbeat every 5 seconds — no lockdown
- **WHEN** a device has an active SSE connection AND sends `POST /api/telemetry` every ≤5 seconds
- **THEN** `heartbeat_miss_count[device_id]` stays at 0
- **THEN** no `data_corrupted` event is sent

#### Scenario: Device stops sending — lockdown after 5 misses
- **WHEN** a device has an active SSE connection AND stops sending telemetry for 25 seconds (5 consecutive watchdog ticks)
- **THEN** the watchdog sends `{ "type": "data_corrupted" }` to all SSE connections of that device
- **THEN** `heartbeat_miss_count[device_id]` is reset to 0

#### Scenario: Device reconnects SSE after lockdown — miss count reset
- **WHEN** `heartbeat_miss_count[device_id]` reaches 5 and fires, then device sends a new telemetry report
- **THEN** `heartbeat_miss_count[device_id]` = 0
- **THEN** watchdog does not fire again unless another 5-miss cycle occurs

#### Scenario: Device disconnects SSE — watchdog ignores it
- **WHEN** a device's SSE connection closes (removed from `active_sse`)
- **THEN** the watchdog skips that device_id on the next tick (no `data_corrupted` sent for offline devices)
- **THEN** stale entries in `last_heartbeat` and `heartbeat_miss_count` for that device_id are harmless (cleaned up lazily or on next SSE connect)

#### Scenario: Graceful shutdown
- **WHEN** the server receives shutdown signal
- **THEN** the watchdog task exits cleanly without sending any `data_corrupted` events
