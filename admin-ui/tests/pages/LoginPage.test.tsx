import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    checkStatus: vi.fn(),
    login: vi.fn(),
    // 公共 /health 端点：未登录 createResource 在 mount 时拉取(version/uptime/availability/dbSize)
    health: vi.fn(),
    // 已登录 token 校验路径(本测试 token=null,不触发,但 page 引用故须存根)
    getHealth: vi.fn(),
  },
}));

vi.mock('@/api/health', () => ({
  healthApi: {
    getStatus: vi.fn(() => Promise.resolve({
      status: 'healthy',
      uptimeSecs: 3600,
      services: {
        store: { healthy: true },
        amas: { healthy: true },
        sse: { healthy: true },
        wordbookCenter: { healthy: true, url: null },
      },
    })),
  },
}));

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getAdminToken: vi.fn(() => null),
    setAdminToken: vi.fn(),
    clearAdminToken: vi.fn(),
    getToken: vi.fn(() => null),
    isAuthenticated: vi.fn(() => false),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('LoginPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAdminApi.checkStatus.mockResolvedValue({ initialized: true });
    mockAdminApi.health.mockResolvedValue({
      status: 'healthy',
      version: '1.1.0',
      uptimeSecs: 90000,
      dbSizeBytes: 5 * 1024 ** 2,
      availability: { pct: 99.95, effectiveSecs: 7 * 86400, totalRequests: 1200 },
    });
    mockAdminApi.getHealth.mockResolvedValue({});
  });

  async function renderPage() {
    const { default: LoginPage } = await import('@/pages/LoginPage');
    return renderWithProviders(() => <LoginPage />);
  }

  it('shows "登录管理后台" heading', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('登录管理后台')).toBeInTheDocument();
    });
  });

  it('shows email input field with label "管理员邮箱"', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByLabelText('管理员邮箱')).toBeInTheDocument();
    });
  });

  it('shows password input field with label "密码"', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByLabelText('密码')).toBeInTheDocument();
    });
  });

  it('shows submit button "进入管理后台"', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /进入管理后台/ })).toBeInTheDocument();
    });
  });

  it('calls checkStatus on mount', async () => {
    await renderPage();
    await waitFor(() => {
      expect(mockAdminApi.checkStatus).toHaveBeenCalled();
    });
  });
});
