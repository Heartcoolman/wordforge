import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getHealth: vi.fn(),
    getDatabase: vi.fn(),
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
import { amasApi } from '@/api/amas';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmasApi = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockHealth = { status: 'ok', dbSizeBytes: 1048576, uptime: 3600, version: '1.0.0' };
const mockDb = { tables: 5, totalSize: 2097152, entries: 10000 };
const mockMonitoring = [{ event: 'test', timestamp: '2026-01-01T00:00:00Z' }];

describe('MonitoringPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: MonitoringPage } = await import('@/pages/admin/MonitoringPage');
    return renderWithProviders(() => <MonitoringPage />);
  }

  it('shows "系统监控" heading', async () => {
    mockAdminApi.getHealth.mockResolvedValue(mockHealth);
    mockAdminApi.getDatabase.mockResolvedValue(mockDb);
    mockAmasApi.getMonitoring.mockResolvedValue(mockMonitoring);
    await renderPage();
    expect(screen.getByText('系统监控')).toBeInTheDocument();
  });

  it('shows loading spinner initially', async () => {
    mockAdminApi.getHealth.mockReturnValue(new Promise(() => {}));
    mockAdminApi.getDatabase.mockReturnValue(new Promise(() => {}));
    mockAmasApi.getMonitoring.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows "系统健康" section heading after loading', async () => {
    mockAdminApi.getHealth.mockResolvedValue(mockHealth);
    mockAdminApi.getDatabase.mockResolvedValue(mockDb);
    mockAmasApi.getMonitoring.mockResolvedValue(mockMonitoring);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('系统健康')).toBeInTheDocument();
    });
  });

  it('shows "数据库信息" section heading after loading', async () => {
    mockAdminApi.getHealth.mockResolvedValue(mockHealth);
    mockAdminApi.getDatabase.mockResolvedValue(mockDb);
    mockAmasApi.getMonitoring.mockResolvedValue(mockMonitoring);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('数据库信息')).toBeInTheDocument();
    });
  });

  it('shows "AMAS 监控事件" section heading after loading', async () => {
    mockAdminApi.getHealth.mockResolvedValue(mockHealth);
    mockAdminApi.getDatabase.mockResolvedValue(mockDb);
    mockAmasApi.getMonitoring.mockResolvedValue(mockMonitoring);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('AMAS 监控事件')).toBeInTheDocument();
    });
  });

  it('displays JSON data in pre elements after loading', async () => {
    mockAdminApi.getHealth.mockResolvedValue(mockHealth);
    mockAdminApi.getDatabase.mockResolvedValue(mockDb);
    mockAmasApi.getMonitoring.mockResolvedValue(mockMonitoring);
    await renderPage();
    await waitFor(() => {
      const preElements = document.querySelectorAll('pre');
      expect(preElements.length).toBe(3);
    });
  });
});
