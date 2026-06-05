/**
 * 数据探针看板 API client。
 *
 * 与 `src/routes/admin/probe_telemetry.rs`（挂 /api/admin/probe-telemetry）对齐。
 * 全部走 admin token；http.ts 自动 unwrap ok() 外层。
 */

import { api } from '@/api/http';
import type {
  ProbeOverview,
  ProbesResponse,
  SamplingResponse,
  SamplingPatchResult,
  SinksResponse,
  SchemaSample,
  AuditResponse,
  StreamResponse,
} from '@/types/probeTelemetry';

const BASE = '/api/admin/probe-telemetry';

export const probeTelemetryApi = {
  overview: (days: number) =>
    api.get<ProbeOverview>(`${BASE}/overview`, { days }, { useAdminToken: true }),

  probes: (days: number) =>
    api.get<ProbesResponse>(`${BASE}/probes`, { days }, { useAdminToken: true }),

  sampling: () =>
    api.get<SamplingResponse>(`${BASE}/sampling`, undefined, { useAdminToken: true }),

  /** locked 行改 rate → 409 SAMPLING_RULE_LOCKED；两者都空 → 400 EMPTY_PATCH；
   *  rate 越界 → 400 SAMPLING_RATE_OUT_OF_RANGE。locked 行仍可改 enabled。 */
  updateSamplingRule: (eventType: string, body: { sampleRate?: number; enabled?: boolean }) =>
    api.patch<SamplingPatchResult>(
      `${BASE}/sampling/${encodeURIComponent(eventType)}`,
      body,
      { useAdminToken: true },
    ),

  sinks: () =>
    api.get<SinksResponse>(`${BASE}/sinks`, undefined, { useAdminToken: true }),

  schema: (eventType: string) =>
    api.get<SchemaSample>(`${BASE}/schema`, { eventType }, { useAdminToken: true }),

  audit: (limit = 20) =>
    api.get<AuditResponse>(`${BASE}/audit`, { limit }, { useAdminToken: true }),

  stream: (limit = 30) =>
    api.get<StreamResponse>(`${BASE}/stream`, { limit }, { useAdminToken: true }),
};
