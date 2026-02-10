import { api } from './client';
import type { LearningRecord, CreateRecordRequest, RecordResponse, RecordStatistics, EnhancedStatistics } from '@/types/record';

export const recordsApi = {
  list: (params?: { limit?: number; offset?: number }) =>
    api.get<LearningRecord[]>('/api/records', params),
  create: (data: CreateRecordRequest) =>
    api.post<RecordResponse>('/api/records', data),
  batchCreate: (records: CreateRecordRequest[]) =>
    api.post<{ count: number; items: RecordResponse[] }>('/api/records/batch', { records }),
  statistics: () =>
    api.get<RecordStatistics>('/api/records/statistics'),
  enhancedStatistics: () =>
    api.get<EnhancedStatistics>('/api/records/statistics/enhanced'),
};
