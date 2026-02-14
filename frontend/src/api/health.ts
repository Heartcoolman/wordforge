import { api } from './client';
import type { AmasMetrics } from '@/types/amas';

export interface PublicHealthStatus {
  status: string;
  uptimeSecs: number;
  store: {
    healthy: boolean;
  };
}

export interface DatabaseHealthStatus {
  healthy: boolean;
  latencyUs: number;
  consecutiveFailures: number;
}

export interface HealthMetricsResponse {
  algorithms: AmasMetrics;
}

export const healthApi = {
  getStatus() {
    return api.get<PublicHealthStatus>('/health');
  },

  async getLiveness() {
    await api.get<void>('/health/live');
    return { live: true };
  },

  async getReadiness() {
    await api.get<void>('/health/ready');
    return { ready: true };
  },

  getDatabase() {
    return api.get<DatabaseHealthStatus>('/health/database', undefined, { useAdminToken: true });
  },

  getMetrics() {
    return api.get<HealthMetricsResponse>('/health/metrics', undefined, { useAdminToken: true });
  },
};
