import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

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
vi.mock('@/api/amas', () => ({ amasApi: { getMetrics: vi.fn() } }));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => null }));
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
    services: {
      amas: { healthy: true },
      sse: { healthy: true, activeConnections: 0, activeDevices: 0 },
      wordbookCenter: { healthy: true },
    },
    ...over,
  };
}

// 仅 health 因用例而异;其余 fetcher 给空数据让页面正常退出 loading
function prime(health: Record<string, unknown> | null) {
  mockAdmin.monitoringRequests.mockResolvedValue(null);
  mockAdmin.monitoringLogs.mockResolvedValue({ logs: [] });
  mockAdmin.monitoringEvents.mockResolvedValue({ events: [] });
  mockAdmin.getHealth.mockResolvedValue(health);
  mockAdmin.monitoringWorkers.mockResolvedValue({ workers: [] });
  mockAdmin.amasAdvisorCost.mockResolvedValue(null);
  mockAdmin.checkUpdate.mockResolvedValue(null);
  mockAmas.getMetrics.mockResolvedValue(null);
}

async function renderPage() {
  const { default: Page } = await import('@/pages/MonitoringPage');
  return renderWithProviders(() => <Page />);
}

describe('MonitoringPage — 服务状态降级 & 空态', () => {
  beforeEach(() => vi.clearAllMocks());

  it('AMAS 子服务不健康时渲染 degraded chip', async () => {
    prime(healthWith({ services: {
      amas: { healthy: false },
      sse: { healthy: true, activeConnections: 0, activeDevices: 0 },
      wordbookCenter: { healthy: true },
    } }));
    await renderPage();
    await waitFor(() => expect(screen.getByText('degraded')).toBeInTheDocument());
  });

  it('health.status=down 时 SQLite 行渲染 down chip', async () => {
    prime(healthWith({ status: 'down' }));
    await renderPage();
    await waitFor(() => expect(screen.getByText('down')).toBeInTheDocument());
  });

  it('无日志 / 无告警时渲染空态文案', async () => {
    prime(healthWith({}));
    await renderPage();
    await waitFor(() => expect(screen.getByText(/暂无日志/)).toBeInTheDocument());
    expect(screen.getByText('暂无告警')).toBeInTheDocument();
  });
});
