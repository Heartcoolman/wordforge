## MODIFIED Requirements

### Requirement: Public health endpoint returns sub-service statuses
`GET /health` response SHALL be extended to include granular sub-service health checks:

Current response: `{ "status": "ok", "uptimeSecs": N, "store": { "healthy": bool } }`

Extended response:
```json
{
  "status": "ok",
  "uptimeSecs": 12345,
  "services": {
    "store": { "healthy": true },
    "amas": { "healthy": true },
    "sse": { "healthy": true },
    "wordbookCenter": { "healthy": true, "url": "https://..." }
  }
}
```

Sub-service health check definitions:
- `store`: existing logic — attempt `get_user_by_id("__health_check__")`, healthy if no panic/error
- `amas`: check that AMAS engine is initialized — `state.amas_engine().is_some()` or equivalent; healthy if engine exists
- `sse`: check that the broadcast channel is open — `state.sse_broadcast().receiver_count() > 0` or sender is alive; always healthy if SSE system is initialized (even with 0 connections)
- `wordbookCenter`: if `wordbookCenterUrl` is configured in settings, attempt HTTP HEAD with 3s timeout; healthy if 2xx; if URL not configured, healthy = true with `url: null`

Overall `status` logic: "ok" if all services healthy; "degraded" if any one service unhealthy; "down" if store is unhealthy.

The `store` field at top level SHALL be removed (moved into `services.store`).

Frontend type update — `PublicHealthStatus`:
```typescript
interface PublicHealthStatus {
  status: string;
  uptimeSecs: number;
  services: {
    store: { healthy: boolean };
    amas: { healthy: boolean };
    sse: { healthy: boolean };
    wordbookCenter: { healthy: boolean; url: string | null };
  };
}
```

#### Scenario: All services healthy
- **WHEN** store, amas, sse, and wordbook-center all pass health checks
- **THEN** `status` = "ok", all services `healthy: true`

#### Scenario: Wordbook center unreachable
- **WHEN** HEAD request to wordbook-center URL times out or returns 5xx
- **THEN** `services.wordbookCenter.healthy` = false, overall `status` = "degraded"

#### Scenario: Store unhealthy
- **WHEN** store health check fails
- **THEN** `services.store.healthy` = false, overall `status` = "down"

#### Scenario: Wordbook center URL not configured
- **WHEN** no `wordbookCenterUrl` in system settings
- **THEN** `services.wordbookCenter` = `{ healthy: true, url: null }`

#### Scenario: PBT — status derivation is a pure function of service health
- **WHEN** given any combination of (store, amas, sse, wordbookCenter) healthy booleans
- **THEN** `status = "down"` iff `store.healthy = false`; `status = "ok"` iff all four healthy; `status = "degraded"` iff `store.healthy = true` AND at least one other is false

#### Scenario: PBT — response always contains exactly 4 services
- **WHEN** `GET /health` is called
- **THEN** `services` contains exactly keys: `store`, `amas`, `sse`, `wordbookCenter`; no extra, no missing

#### Scenario: PBT — uptimeSecs is monotonically non-decreasing
- **WHEN** two requests are made at t1 < t2
- **THEN** `uptimeSecs(t2) ≥ uptimeSecs(t1)`

#### Scenario: PBT — idempotency of status and services
- **WHEN** no dependency state changes between two requests
- **THEN** `status` and `services` fields are identical; only `uptimeSecs` may differ
