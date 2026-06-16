import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 重设计后的 MonitoringPage 调用的全部 adminApi 方法（含 getDatabase / 死信运维）。
vi.mock('@/api/admin', () => ({
  adminApi: {
    monitoringRequests: vi.fn(),
    monitoringLogs: vi.fn(),
    monitoringEvents: vi.fn(),
    getHealth: vi.fn(),
    getDatabase: vi.fn(),
    monitoringWorkers: vi.fn(),
    amasAdvisorCost: vi.fn(),
    checkUpdate: vi.fn(),
    monitoringDeadLetter: vi.fn(),
    monitoringDeadLetterRequeue: vi.fn(),
    monitoringDeadLetterPurge: vi.fn(),
  },
}));
vi.mock('@/api/amas', () => ({ amasApi: { getMetrics: vi.fn() } }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

function healthWith(over: Record<string, unknown>) {
  return {
    status: 'healthy', dbSizeBytes: 1024, uptimeSecs: 100, version: '1.1.0', errorRate: 0,
    storeProbeOk: true,
    services: {
      amas: { healthy: true },
      sse: { healthy: true, activeConnections: 0, activeDevices: 0 },
      wordbookCenter: { healthy: true },
    },
    ...over,
  };
}

function emptyRequests() {
  return {
    windowSecs: 3600, effectiveSecs: 3600, totalRequests: 0, total5xx: 0,
    qpsAvg: 0, p50Ms: 0, p99Ms: 0, errorRate: 0, availabilityPct: 100,
    bandwidthInBps: 0, series: [] as { t: number; qps: number; p99Ms: number }[],
  };
}

function emptyDatabase() {
  return {
    sizeOnDisk: 1024, tableCount: 0, tables: [] as string[],
    pageSize: 4096, pageCount: 1, walEnabled: true,
  };
}

// 仅 health 因用例而异;其余 fetcher 给安全默认值让页面正常退出 loading
function prime(health: Record<string, unknown> | null) {
  mockAdmin.monitoringRequests.mockResolvedValue(emptyRequests());
  mockAdmin.monitoringLogs.mockResolvedValue({ logs: [] });
  mockAdmin.monitoringEvents.mockResolvedValue({ events: [] });
  mockAdmin.getHealth.mockResolvedValue(health);
  mockAdmin.getDatabase.mockResolvedValue(emptyDatabase());
  mockAdmin.monitoringWorkers.mockResolvedValue({ workers: [] });
  mockAdmin.amasAdvisorCost.mockResolvedValue(null);
  mockAdmin.checkUpdate.mockResolvedValue(null);
  mockAdmin.monitoringDeadLetter.mockResolvedValue({ entries: [] });
  mockAdmin.monitoringDeadLetterRequeue.mockResolvedValue({ requeued: true, id: 1 });
  mockAdmin.monitoringDeadLetterPurge.mockResolvedValue({ purged: true, id: 1 });
  mockAmas.getMetrics.mockResolvedValue(null);
}

async function renderPage() {
  const { default: Page } = await import('@/pages/MonitoringPage');
  return renderWithProviders(() => <Page />);
}

describe('MonitoringPage — 服务状态降级 & 空态', () => {
  beforeEach(() => vi.clearAllMocks());

  it('AMAS 子服务不健康时仍渲染服务状态面板与 AMAS 引擎行', async () => {
    prime(healthWith({ services: {
      amas: { healthy: false },
      sse: { healthy: true, activeConnections: 0, activeDevices: 0 },
      wordbookCenter: { healthy: true },
    } }));
    await renderPage();
    // 服务状态面板渲染，AMAS 引擎行存在；HTTP 服务因 status=healthy 显示运行正常
    await waitFor(() => expect(screen.getByText('服务状态')).toBeInTheDocument());
    expect(screen.getByText('AMAS 引擎')).toBeInTheDocument();
    expect(screen.getByText('运行正常')).toBeInTheDocument();
  });

  it('health.status=down 时 HTTP 服务行渲染“不可用”', async () => {
    prime(healthWith({ status: 'down' }));
    await renderPage();
    await waitFor(() => expect(screen.getByText('不可用')).toBeInTheDocument());
    expect(screen.getByText('HTTP 服务')).toBeInTheDocument();
  });

  it('无日志 / 无告警时渲染空态文案', async () => {
    prime(healthWith({}));
    await renderPage();
    await waitFor(() => expect(screen.getByText(/暂无日志/)).toBeInTheDocument());
    expect(screen.getByText('暂无告警')).toBeInTheDocument();
  });
});
