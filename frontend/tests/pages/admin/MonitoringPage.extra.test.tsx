import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: { getHealth: vi.fn(), getDatabase: vi.fn() },
}));
vi.mock('@/api/health', () => ({
  healthApi: { getStatus: vi.fn() },
}));
vi.mock('@/api/amas', () => ({
  amasApi: { getMonitoring: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { healthApi } from '@/api/health';
import { amasApi } from '@/api/amas';
const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockHealth = healthApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('MonitoringPage extra branches', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/admin/MonitoringPage');
    return renderWithProviders(() => <Page />);
  }

  it('shows allFailed empty when all four fetches reject', async () => {
    mockAdmin.getHealth.mockRejectedValue(new Error('a'));
    mockHealth.getStatus.mockRejectedValue(new Error('b'));
    mockAdmin.getDatabase.mockRejectedValue(new Error('c'));
    mockAmas.getMonitoring.mockRejectedValue(new Error('d'));
    await renderPage();
    await waitFor(() => expect(screen.getByText(/无法获取任何监控数据/)).toBeInTheDocument());
  });

  it('shows partial-error cards when some fetches fail', async () => {
    mockAdmin.getHealth.mockRejectedValue(new Error('health err'));
    mockHealth.getStatus.mockResolvedValue({
      status: 'ok', uptimeSecs: 100,
      services: { store: { healthy: true } },
    });
    mockAdmin.getDatabase.mockRejectedValue(new Error('db err'));
    mockAmas.getMonitoring.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/系统健康: health err/)).toBeInTheDocument());
    expect(screen.getByText(/数据库信息: db err/)).toBeInTheDocument();
  });

  it('renders degraded health badge', async () => {
    mockAdmin.getHealth.mockResolvedValue({ status: 'degraded', dbSizeBytes: 1024, uptimeSecs: 86460, version: '1' });
    mockHealth.getStatus.mockResolvedValue({ status: 'ok', uptimeSecs: 100, services: {} });
    mockAdmin.getDatabase.mockResolvedValue({ sizeOnDisk: 2048, tableCount: 1, tables: ['x'], pageSize: 4096, pageCount: 1, walEnabled: false });
    mockAmas.getMonitoring.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('性能降级')).toBeInTheDocument());
    expect(screen.getByText('未启用')).toBeInTheDocument();
  });

  it('renders down status with fallback label', async () => {
    mockAdmin.getHealth.mockResolvedValue({ status: 'unknown-xyz', dbSizeBytes: 1024, uptimeSecs: 100, version: '1' });
    mockHealth.getStatus.mockResolvedValue({ status: 'down', uptimeSecs: 100, services: { x: { healthy: false } } });
    mockAdmin.getDatabase.mockResolvedValue({ sizeOnDisk: 0, tableCount: 0, tables: [], pageSize: 4096, pageCount: 0, walEnabled: true });
    mockAmas.getMonitoring.mockResolvedValue([{ eventType: 't', timestamp: '2026-01-01T00:00:00Z', data: null }]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('服务异常')).toBeInTheDocument());
    expect(screen.getByText('异常')).toBeInTheDocument();
  });

  it('renders array data in events', async () => {
    mockAdmin.getHealth.mockResolvedValue({ status: 'healthy', dbSizeBytes: 1024, uptimeSecs: 100, version: '1' });
    mockHealth.getStatus.mockResolvedValue({ status: 'ok', uptimeSecs: 100, services: {} });
    mockAdmin.getDatabase.mockResolvedValue({ sizeOnDisk: 0, tableCount: 0, tables: [], pageSize: 4096, pageCount: 0, walEnabled: true });
    mockAmas.getMonitoring.mockResolvedValue([{ eventType: 'arr', timestamp: '2026-01-01T00:00:00Z', data: [1, 2, 3] }]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('1, 2, 3')).toBeInTheDocument());
  });

  it('renders empty array data', async () => {
    mockAdmin.getHealth.mockResolvedValue({ status: 'healthy', dbSizeBytes: 1024, uptimeSecs: 100, version: '1' });
    mockHealth.getStatus.mockResolvedValue({ status: 'ok', uptimeSecs: 100, services: {} });
    mockAdmin.getDatabase.mockResolvedValue({ sizeOnDisk: 0, tableCount: 0, tables: [], pageSize: 4096, pageCount: 0, walEnabled: true });
    mockAmas.getMonitoring.mockResolvedValue([{ eventType: 'arr', timestamp: '2026-01-01T00:00:00Z', data: [] }]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('[]')).toBeInTheDocument());
  });

  it('shows empty events message', async () => {
    mockAdmin.getHealth.mockResolvedValue({ status: 'healthy', dbSizeBytes: 1024, uptimeSecs: 100, version: '1' });
    mockHealth.getStatus.mockResolvedValue({ status: 'ok', uptimeSecs: 100, services: {} });
    mockAdmin.getDatabase.mockResolvedValue({ sizeOnDisk: 0, tableCount: 0, tables: [], pageSize: 4096, pageCount: 0, walEnabled: true });
    mockAmas.getMonitoring.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无事件')).toBeInTheDocument());
  });
});
