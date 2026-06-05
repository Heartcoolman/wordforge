import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 新版监控页(系统监控终端式看板)调用的 API 全集
vi.mock('@/api/admin', () => ({
  adminApi: {
    monitoringRequests: vi.fn(),
    monitoringLogs: vi.fn(),
    monitoringEvents: vi.fn(),
    getHealth: vi.fn(),
    monitoringWorkers: vi.fn(),
    amasAdvisorCost: vi.fn(),
    checkUpdate: vi.fn(),
  },
}));
vi.mock('@/api/amas', () => ({
  amasApi: { getMetrics: vi.fn() },
}));
// EChart 依赖 canvas,jsdom 下不渲染;桩为 no-op 不影响区块结构断言
vi.mock('@/components/ui/EChart', () => ({ EChart: () => null }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';

const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockRequests = {
  windowSecs: 3600, effectiveSecs: 3600, totalRequests: 1200, total5xx: 1,
  qpsAvg: 0.33, p50Ms: 12, p99Ms: 88, errorRate: 0.0008, availabilityPct: 99.92,
  bandwidthInBps: 2048, series: [],
};
const mockHealth = {
  status: 'healthy' as const, dbSizeBytes: 1048576, uptimeSecs: 3600, version: '1.1.0', errorRate: 0,
  services: {
    amas: { healthy: true },
    sse: { healthy: true, activeConnections: 2, activeDevices: 2, maxConnections: 256 },
    wordbookCenter: { healthy: true },
  },
};
const mockLogs = { logs: [{ tsMs: 1735689600000, level: 'INFO', target: 'http', message: 'request handled' }] };
const mockEvents = { events: [{ tsMs: 1735689600000, severity: 'warning' as const, title: 'worker 延迟', desc: 'llm_advisor 超时' }] };
const mockWorkers = { workers: [] };
const mockCost = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 42.1, forecastYuan: 6.84, avg7dCostYuan: 0.14,
  monthCalls: 31, acceptedCount: 47, rejectedCount: 6, acceptanceRate: 0.887, usdToCny: 7.3,
};
const mockUpdate = { currentVersion: '1.1.0', latestVersion: '1.1.0', hasUpdate: false, releaseUrl: null, releaseNotes: null };

function primeMocks() {
  mockAdmin.monitoringRequests.mockResolvedValue(mockRequests);
  mockAdmin.monitoringLogs.mockResolvedValue(mockLogs);
  mockAdmin.monitoringEvents.mockResolvedValue(mockEvents);
  mockAdmin.getHealth.mockResolvedValue(mockHealth);
  mockAdmin.monitoringWorkers.mockResolvedValue(mockWorkers);
  mockAdmin.amasAdvisorCost.mockResolvedValue(mockCost);
  mockAdmin.checkUpdate.mockResolvedValue(mockUpdate);
  mockAmas.getMetrics.mockResolvedValue(null);
}

async function renderPage() {
  const { default: MonitoringPage } = await import('@/pages/MonitoringPage');
  return renderWithProviders(() => <MonitoringPage />);
}

describe('MonitoringPage', () => {
  beforeEach(() => vi.clearAllMocks());

  it('初始展示加载 spinner', async () => {
    // 全部 fetcher 挂起 → loading 保持 true
    for (const k of ['monitoringRequests', 'monitoringLogs', 'monitoringEvents', 'getHealth', 'monitoringWorkers', 'amasAdvisorCost', 'checkUpdate']) {
      mockAdmin[k].mockReturnValue(new Promise(() => {}));
    }
    mockAmas.getMetrics.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('加载完成后渲染页头「系统监控」', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统监控')).toBeInTheDocument());
  });

  it('渲染服务状态 / Worker 心跳 / 请求与延迟 / 实时日志 / 告警时间线 区块', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('服务状态')).toBeInTheDocument());
    expect(screen.getByText('Worker 心跳')).toBeInTheDocument();
    expect(screen.getByText(/请求与延迟/)).toBeInTheDocument();
    expect(screen.getByText('实时日志')).toBeInTheDocument();
    expect(screen.getByText('告警时间线')).toBeInTheDocument();
  });

  it('渲染实时日志行内容', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('[http] request handled')).toBeInTheDocument());
  });

  it('渲染告警事件标题', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('worker 延迟')).toBeInTheDocument());
  });
});
