## MODIFIED Requirements

### Requirement: Client reports telemetry every 5 seconds with enriched payload
The authenticated client SHALL call `POST /api/telemetry` every **5 seconds** (changed from 5 minutes). The worker activates only when the user is authenticated. The `INTERVAL_MS` constant in `frontend/src/workers/telemetry.ts` MUST be set to `5000`.

The payload schema is extended to include two sub-objects: `device` (static hardware/software fingerprint) and `behavior` (delta increments since last report). All original top-level fields (`sessionDurationSecs`, `actionsPerMin`, `featureUsage`, `errorCount`, `avgResponseTimeMs`) are retained for backward compatibility.

#### New eventType: `session_start`
A new `eventType` value `"session_start"` is introduced. The worker SHALL call `POST /api/telemetry` with `eventType: "session_start"` immediately when `startTelemetryWorker()` is called (i.e., when the user is authenticated and SSE connects). This initial report MUST include the full `device` object.

Subsequent `periodic` reports MAY omit the `device` object. The server SHALL accept both forms.

#### Device fingerprint fields (collected via `collectDeviceFingerprint()` in `frontend/src/lib/device.ts`)
```typescript
interface DeviceFingerprint {
  cpuCores: number;           // navigator.hardwareConcurrency
  memoryGb: number | null;    // navigator.deviceMemory ?? null
  screenWidth: number;        // screen.width
  screenHeight: number;       // screen.height
  pixelRatio: number;         // devicePixelRatio
  osName: string;             // parsed from navigator.userAgent
  browserName: string;        // parsed from navigator.userAgent
  browserVersion: string;     // parsed from navigator.userAgent
  timezone: string;           // Intl.DateTimeFormat().resolvedOptions().timeZone
  language: string;           // navigator.language
  touchSupport: boolean;      // navigator.maxTouchPoints > 0
  onlineStatus: boolean;      // navigator.onLine
}
```

#### Behavior increment fields (accumulated between reports)
```typescript
interface BehaviorDelta {
  currentRoute: string;       // current window.location.pathname
  clickCount: number;         // total clicks since last report
  clickTargets: Array<{ label: string; tag: string }>; // last ≤20 clicks
  scrollDepthPct: number;     // max scroll % on current page since last report
  visibilityChanges: number;  // document visibility change events since last report
  routeChanges: number;       // route navigation events since last report
}
```

`clickTargets` is capped at 20 entries. Each entry captures the **nearest interactive ancestor** of `event.target` (traversing up the DOM to the first element matching `button`, `a`, `input`, `select`, `textarea`, or `[role]`; fall back to `event.target` itself if none found). `tag` is that element's `tagName` (lowercased). `label` is derived from (in priority order): `aria-label` attribute → `title` attribute → `innerText` (truncated to 50 chars) → empty string.

`visibilityChanges` is incremented by 1 on every `document.visibilitychange` event, regardless of direction (hidden→visible or visible→hidden). One complete foreground↔background round trip counts as 2.

`scrollDepthPct` tracks the maximum scroll percentage on the **current page** since the last report. When a route change occurs, `scrollDepthPct` MUST be reset to 0 immediately before updating `currentRoute`.

#### Behavior counter buffer swap (prevents data loss during HTTP roundtrip)
Before initiating each `POST /api/telemetry` request, the worker MUST:
1. Snapshot the current `behavior` counters into the request payload
2. Immediately replace the active counters with a fresh zero-value buffer
3. Accumulate new events into the new buffer while the HTTP request is in flight
4. On success: discard the old snapshot (already sent)
5. On failure: merge the old snapshot back into the new buffer (add counts together) to avoid data loss

#### Full request body schema
```json
{
  "eventType": "periodic" | "on_demand" | "session_start",
  "requestId": "<uuid> | null",
  "clientTs": "<ISO8601 UTC>",
  "payload": {
    "device": { ... },
    "behavior": { ... },
    "sessionDurationSecs": <integer ≥ 0>,
    "actionsPerMin": <float ≥ 0>,
    "featureUsage": { "<feature>": <integer ≥ 0> },
    "errorCount": <integer ≥ 0>,
    "avgResponseTimeMs": <float ≥ 0>
  }
}
```

The server-side `TelemetryRequest` struct in `src/routes/telemetry.rs` accepts the extended payload as `serde_json::Value` (no structural change to Rust struct needed; classification happens in a separate processing step after storage).

#### Scenario: session_start sent on worker start
- **WHEN** `startTelemetryWorker()` is called (user authenticated + SSE active)
- **THEN** the worker immediately calls `POST /api/telemetry` with `eventType: "session_start"`, full `device` object, zero-value `behavior` delta
- **THEN** the periodic interval timer starts; subsequent calls use `eventType: "periodic"`

#### Scenario: Periodic 5-second heartbeat
- **WHEN** 5 seconds have elapsed since the last upload AND the user is authenticated
- **THEN** the client sends `POST /api/telemetry` with `eventType: "periodic"`, `behavior` delta for that 5-second window, accumulated click/scroll/route data
- **THEN** the worker resets `behavior` counters to zero after successful send

#### Scenario: Click tracking
- **WHEN** user clicks any element in the document
- **THEN** `clickCount` increments by 1
- **THEN** if `clickTargets.length < 20`, an entry `{ label, tag }` is appended; if already 20 entries, no new entry is added (count still increments)

#### Scenario: Route change tracking
- **WHEN** the SPA navigates to a new route (via history.pushState or popstate)
- **THEN** `routeChanges` increments by 1
- **THEN** `currentRoute` is updated to the new `window.location.pathname`

#### Scenario: Backward compatibility — server accepts payload without device/behavior
- **WHEN** client submits payload that lacks `device` or `behavior` sub-objects
- **THEN** the server stores the record without error (these fields are optional in the JSON schema)
