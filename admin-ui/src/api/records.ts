import { api } from './client';
import type { LearningRecord, CreateRecordRequest, RecordResponse, RecordStatistics, EnhancedStatistics } from '@/types/record';
import type { PaginatedResponse } from '@/types/api';

export interface BatchCreateRecordResponse {
  count: number;
  failed: number;
  partial: boolean;
  items: RecordResponse[];
  errors: Array<{ index: number; code: string; message: string }>;
}

function sanitizeRecord(data: CreateRecordRequest): CreateRecordRequest {
  return {
    ...data,
    responseTimeMs: Math.round(data.responseTimeMs),
    dwellTimeMs: data.dwellTimeMs != null ? Math.round(data.dwellTimeMs) : undefined,
    pauseCount: data.pauseCount != null ? Math.round(data.pauseCount) : undefined,
    switchCount: data.switchCount != null ? Math.round(data.switchCount) : undefined,
    retryCount: data.retryCount != null ? Math.round(data.retryCount) : undefined,
    focusLossDurationMs: data.focusLossDurationMs != null ? Math.round(data.focusLossDurationMs) : undefined,
    pausedTimeMs: data.pausedTimeMs != null ? Math.round(data.pausedTimeMs) : undefined,
  };
}

export const recordsApi = {
  list: (params?: { page?: number; perPage?: number }) =>
    api.get<PaginatedResponse<LearningRecord>>('/api/records', params),
  create: (data: CreateRecordRequest) =>
    api.post<RecordResponse>('/api/records', sanitizeRecord(data)),
  batchCreate: (records: CreateRecordRequest[]) =>
    api.post<BatchCreateRecordResponse>('/api/records/batch', { records: records.map(sanitizeRecord) }),
  statistics: () =>
    api.get<RecordStatistics>('/api/records/statistics'),
  enhancedStatistics: () =>
    api.get<EnhancedStatistics>('/api/records/statistics/enhanced'),
};
