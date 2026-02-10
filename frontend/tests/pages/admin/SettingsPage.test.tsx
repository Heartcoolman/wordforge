import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
    broadcast: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockSettings = {
  maxUsers: 1000,
  registrationEnabled: true,
  maintenanceMode: false,
  defaultDailyWords: 20,
};

describe('SettingsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: SettingsPage } = await import('@/pages/admin/SettingsPage');
    return renderWithProviders(() => <SettingsPage />);
  }

  it('shows "系统设置" heading', async () => {
    mockAdminApi.getSettings.mockResolvedValue(mockSettings);
    await renderPage();
    expect(screen.getByText('系统设置')).toBeInTheDocument();
  });

  it('shows loading spinner initially', async () => {
    mockAdminApi.getSettings.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows settings form after loading', async () => {
    mockAdminApi.getSettings.mockResolvedValue(mockSettings);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByLabelText('最大用户数')).toBeInTheDocument();
    });
    expect(screen.getByLabelText('默认每日单词数')).toBeInTheDocument();
  });

  it('shows switch labels after loading', async () => {
    mockAdminApi.getSettings.mockResolvedValue(mockSettings);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('开放注册')).toBeInTheDocument();
    });
    expect(screen.getByText('维护模式')).toBeInTheDocument();
  });

  it('shows "保存设置" button after loading', async () => {
    mockAdminApi.getSettings.mockResolvedValue(mockSettings);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '保存设置' })).toBeInTheDocument();
    });
  });

  it('shows "广播消息" section heading', async () => {
    mockAdminApi.getSettings.mockResolvedValue(mockSettings);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('广播消息')).toBeInTheDocument();
    });
  });

  it('shows "发送广播" button', async () => {
    mockAdminApi.getSettings.mockResolvedValue(mockSettings);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '发送广播' })).toBeInTheDocument();
    });
  });
});
