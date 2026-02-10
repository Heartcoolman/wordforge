import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getStats: vi.fn(),
    getHealth: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('AdminDashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: AdminDashboard } = await import('@/pages/admin/AdminDashboard');
    return renderWithProviders(() => <AdminDashboard />);
  }

  it('shows "仪表盘" heading', async () => {
    mockAdminApi.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000 });
    mockAdminApi.getHealth.mockResolvedValue({ status: 'ok', dbSizeBytes: 1048576, uptime: 7200, version: '1.0.0' });
    await renderPage();
    expect(screen.getByText('仪表盘')).toBeInTheDocument();
  });

  it('shows loading spinner initially', async () => {
    mockAdminApi.getStats.mockReturnValue(new Promise(() => {}));
    mockAdminApi.getHealth.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows stat cards with correct values after loading', async () => {
    mockAdminApi.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000 });
    mockAdminApi.getHealth.mockResolvedValue({ status: 'ok', dbSizeBytes: 1048576, uptime: 7200, version: '1.0.0' });
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('注册用户')).toBeInTheDocument();
    });
    expect(screen.getByText('单词总数')).toBeInTheDocument();
    expect(screen.getByText('学习记录')).toBeInTheDocument();
  });

  it('shows "系统状态" section after loading', async () => {
    mockAdminApi.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000 });
    mockAdminApi.getHealth.mockResolvedValue({ status: 'ok', dbSizeBytes: 1048576, uptime: 7200, version: '1.0.0' });
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('系统状态')).toBeInTheDocument();
    });
  });

  it('shows health status details after loading', async () => {
    mockAdminApi.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000 });
    mockAdminApi.getHealth.mockResolvedValue({ status: 'ok', dbSizeBytes: 1048576, uptime: 7200, version: '1.0.0' });
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('ok')).toBeInTheDocument();
    });
    expect(screen.getByText('1.0.0')).toBeInTheDocument();
    expect(screen.getByText('2 小时')).toBeInTheDocument();
  });
});
