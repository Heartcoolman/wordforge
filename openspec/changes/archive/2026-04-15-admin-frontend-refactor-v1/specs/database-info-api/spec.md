## MODIFIED Requirements

### Requirement: Database stats API returns SQLite-specific fields
`GET /api/admin/monitoring/database` response SHALL be extended with SQLite pragma information:

Current response: `{ "sizeOnDisk": N, "tableCount": N, "tables": [...] }`

Extended response:
```json
{
  "sizeOnDisk": 1048576,
  "tableCount": 12,
  "tables": ["users", "learning_records", "..."],
  "pageSize": 4096,
  "pageCount": 256,
  "walEnabled": true
}
```

New fields sourced from SQLite pragmas:
- `pageSize`: result of `PRAGMA page_size` (integer, typically 4096)
- `pageCount`: result of `PRAGMA page_count` (integer)
- `walEnabled`: result of `PRAGMA journal_mode` — true if result equals "wal" (case-insensitive), false otherwise

Frontend type update — `DatabaseInfo`:
```typescript
interface DatabaseInfo {
  sizeOnDisk: number;
  tableCount: number;
  tables: string[];
  pageSize: number;
  pageCount: number;
  walEnabled: boolean;
}
```

#### Scenario: Database returns all fields
- **WHEN** admin calls `GET /api/admin/monitoring/database`
- **THEN** response includes sizeOnDisk, tableCount, tables, pageSize, pageCount, and walEnabled

#### Scenario: WAL mode is enabled
- **WHEN** SQLite journal_mode is "wal"
- **THEN** `walEnabled` = true

#### Scenario: WAL mode is not enabled
- **WHEN** SQLite journal_mode is "delete" or any value other than "wal"
- **THEN** `walEnabled` = false

#### Scenario: PBT — tableCount equals tables array length
- **WHEN** `GET /api/admin/monitoring/database` is called
- **THEN** `tableCount === tables.length` always holds

#### Scenario: PBT — all numeric fields are non-negative integers
- **WHEN** `GET /api/admin/monitoring/database` is called
- **THEN** `sizeOnDisk ≥ 0`, `tableCount ≥ 0`, `pageSize > 0`, `pageCount ≥ 0` and all are integers

#### Scenario: PBT — walEnabled is strictly boolean
- **WHEN** `GET /api/admin/monitoring/database` is called
- **THEN** `walEnabled` is exactly `true` or `false`, never null/string/number

#### Scenario: PBT — idempotency
- **WHEN** no DB schema changes occur between two requests
- **THEN** both responses are identical
