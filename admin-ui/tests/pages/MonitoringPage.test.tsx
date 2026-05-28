import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getHealth: vi.fn(),
    getDatabase: vi.fn(),
  },
}));
vi.mock('@/api/health', () => ({
  healthApi: {
    getStatus: vi.fn(),
  },
}));

vi.mock('@/api/amas', () => ({
  amasApi: {
    getMonitoring: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { healthApi } from '@/api/health';
import { amasApi } from '@/api/amas';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockHealthApi = healthApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmasApi = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockHealth = { status: 'healthy', dbSizeBytes: 1048576, uptimeSecs: 3600, version: '1.0.0' };
const mockPublicHealth = {
  status: 'ok',
  uptimeSecs: 3600,
  services: {
    store: { healthy: true },
    amas: { healthy: true },
    sse: { healthy: true },
    wordbookCenter: { healthy: true, url: null },
  },
};
const mockDb = { sizeOnDisk: 2097152, tableCount: 5, tables: ['users'], pageSize: 4096, pageCount: 512, walEnabled: true };
const mockMonitoring = [{ eventType: 'test', timestamp: '2026-01-01T00:00:00Z', data: { foo: 'bar' } }];

describe('MonitoringPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function primeMocks() {
    mockAdminApi.getHealth.mockResolvedValue(mockHealth);
    mockHealthApi.getStatus.mockResolvedValue(mockPublicHealth);
    mockAdminApi.getDatabase.mockResolvedValue(mockDb);
    mockAmasApi.getMonitoring.mockResolvedValue(mockMonitoring);
  }

  async function renderPage() {
    const { default: MonitoringPage } = await import('@/pages/MonitoringPage');
    return renderWithProviders(() => <MonitoringPage />);
  }

  it('renders monitoring sections after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('系统健康')).toBeInTheDocument();
    });
  });

  it('shows loading spinner initially', async () => {
    mockAdminApi.getHealth.mockReturnValue(new Promise(() => {}));
    mockHealthApi.getStatus.mockReturnValue(new Promise(() => {}));
    mockAdminApi.getDatabase.mockReturnValue(new Promise(() => {}));
    mockAmasApi.getMonitoring.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows "系统健康" section heading after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('系统健康')).toBeInTheDocument();
    });
  });

  it('shows "数据库信息" section heading after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('数据库信息')).toBeInTheDocument();
    });
  });

  it('shows "AMAS 监控事件" section heading after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('AMAS 监控事件')).toBeInTheDocument();
    });
  });

  it('renders monitoring event payload after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('test')).toBeInTheDocument();
      expect(screen.getByText('foo:')).toBeInTheDocument();
      expect(screen.getByText('bar')).toBeInTheDocument();
    });
  });
});
