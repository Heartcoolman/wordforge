## ADDED Requirements

### Requirement: Daily active users time-series API
`GET /api/admin/analytics/daily-active-users` SHALL return daily active user counts for the past N days.

Query parameters:
- `days`: optional integer, default=7, min=1, max=30; values outside range → HTTP 400 with `{ "code": "INVALID_DAYS", "message": "days must be between 1 and 30" }`
- Non-integer / negative / zero → same 400 response

Response (wrapped in standard `{ "success": true, "data": [...] }`):
```json
[
  { "date": "2026-04-09", "count": 12 },
  { "date": "2026-04-10", "count": 15 }
]
```

Rules:
- Dates in UTC, format `YYYY-MM-DD`, ascending order
- "Active" defined as: user has at least one `learning_records` entry on that date (reuses existing `count_active_users_since` logic, applied per-day via GROUP BY)
- Days with zero active users SHALL be included with `count: 0` (no gaps)
- Requires admin authentication (AdminAuthUser extractor)

Frontend API method: `adminApi.getDailyActiveUsers(days?: number)` in `frontend/src/api/admin.ts`.

#### Scenario: Default request without days param
- **WHEN** `GET /api/admin/analytics/daily-active-users` is called without `days`
- **THEN** response contains 7 entries covering the past 7 days in ascending date order

#### Scenario: Custom days param
- **WHEN** `GET /api/admin/analytics/daily-active-users?days=14`
- **THEN** response contains 14 entries

#### Scenario: Invalid days param
- **WHEN** `GET /api/admin/analytics/daily-active-users?days=0` or `?days=31` or `?days=abc`
- **THEN** HTTP 400 with code `INVALID_DAYS`

#### Scenario: Days with no activity
- **WHEN** no users were active on 2026-04-11
- **THEN** the entry `{ "date": "2026-04-11", "count": 0 }` is present in the response array

#### Scenario: PBT — array length equals days parameter
- **WHEN** admin requests with any valid `days` value N (1 ≤ N ≤ 30)
- **THEN** the response array has exactly N elements

#### Scenario: PBT — dates are contiguous ascending UTC days
- **WHEN** response contains N entries
- **THEN** for each consecutive pair (i, i+1), `date[i+1] = date[i] + 1 day`, all in `YYYY-MM-DD` format, and `date[N-1]` equals today's UTC date

#### Scenario: PBT — distinct user counting
- **WHEN** the same user_id has K records (K ≥ 2) on the same date
- **THEN** that user is counted only once for that date's `count`

#### Scenario: PBT — idempotency
- **WHEN** the same request is made twice with no DB changes between calls
- **THEN** both responses are byte-identical

---

### Requirement: Daily learning records time-series API
`GET /api/admin/analytics/daily-records` SHALL return daily learning record statistics for the past N days.

Query parameters: same as daily-active-users (`days`, default=7, range 1–30, same 400 validation).

Response (wrapped in standard envelope):
```json
[
  { "date": "2026-04-09", "correct": 340, "total": 410 },
  { "date": "2026-04-10", "correct": 290, "total": 350 }
]
```

Rules:
- Dates in UTC, `YYYY-MM-DD`, ascending order
- `total`: count of all `learning_records` on that date
- `correct`: count of records where `is_correct = 1` on that date
- Days with zero records SHALL be included with `correct: 0, total: 0`
- Requires admin authentication

Frontend API method: `adminApi.getDailyRecords(days?: number)`.

#### Scenario: Default request
- **WHEN** `GET /api/admin/analytics/daily-records` is called
- **THEN** response contains 7 entries with correct/total counts per day

#### Scenario: Empty day
- **WHEN** no records exist for 2026-04-12
- **THEN** entry is `{ "date": "2026-04-12", "correct": 0, "total": 0 }`

#### Scenario: PBT — correct ≤ total for every entry
- **WHEN** response contains any entry
- **THEN** `0 ≤ entry.correct ≤ entry.total` is always true

#### Scenario: PBT — sum consistency across window
- **WHEN** the query window contains R total records and C correct records in the database
- **THEN** `Σ entry.total = R` and `Σ entry.correct = C`

#### Scenario: PBT — dates contiguous and ascending
- **WHEN** response contains N entries for valid days=N
- **THEN** dates form a contiguous ascending sequence ending on today's UTC date

---

### Requirement: Existing stats/engagement/learning APIs gain trend fields
Backend SHALL extend three existing API responses with trend comparison data (comparing today vs yesterday):

`GET /api/admin/stats` — add:
```json
{
  "users": 100, "words": 5000, "records": 12000,
  "trend": {
    "users": { "value": 3, "label": "较昨日" },
    "records": { "value": -2, "label": "较昨日" }
  }
}
```
- `trend.users.value`: percentage change in new user registrations (today vs yesterday); 0 if yesterday had none
- `trend.records.value`: percentage change in learning records count (today vs yesterday)
- `words` has no trend (word count is not time-dependent)

`GET /api/admin/analytics/engagement` — add:
```json
{
  "totalUsers": 100, "activeToday": 12, "retentionRate": 0.12,
  "trend": {
    "activeToday": { "value": 20, "label": "较昨日" }
  }
}
```

`GET /api/admin/analytics/learning` — add:
```json
{
  "totalWords": 5000, "totalRecords": 12000, "totalCorrect": 9600, "overallAccuracy": 0.8,
  "trend": {
    "totalRecords": { "value": 5, "label": "较昨日" },
    "overallAccuracy": { "value": -1, "label": "较昨日" }
  }
}
```

Frontend types SHALL be updated to include optional `trend` field.

#### Scenario: Stats API returns trend data
- **WHEN** yesterday had 5 new users and today has 7
- **THEN** `trend.users.value` = 40 (percent increase, rounded to integer)

#### Scenario: Yesterday had zero activity
- **WHEN** yesterday had 0 records and today has 10
- **THEN** `trend.records.value` = 0 (avoid division by zero, report as no change)

#### Scenario: PBT — trend values are finite integers, never NaN/Inf
- **WHEN** any combination of today/yesterday counts (including both zero)
- **THEN** all trend `value` fields are finite integers

#### Scenario: PBT — trend sign consistency
- **WHEN** today's count > yesterday's count (and yesterday > 0)
- **THEN** `trend.value` is positive; when today < yesterday, `trend.value` is negative; when equal, `trend.value` is 0

#### Scenario: PBT — trend formula: pct(t,y) = round(100*(t-y)/y) when y>0, else 0
- **WHEN** yesterday had Y items and today has T items (Y > 0)
- **THEN** `trend.value = round(100 * (T - Y) / Y)`
