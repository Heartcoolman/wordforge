import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    setup: vi.fn(),
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

describe('AdminSetupPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: AdminSetupPage } = await import('@/pages/admin/AdminSetupPage');
    return renderWithProviders(() => <AdminSetupPage />);
  }

  it('shows "初始化管理后台" heading', async () => {
    await renderPage();
    expect(screen.getByText('初始化管理后台')).toBeInTheDocument();
  });

  it('shows description text "首次使用，请创建管理员账户"', async () => {
    await renderPage();
    expect(screen.getByText('首次使用，请创建管理员账户')).toBeInTheDocument();
  });

  it('shows email input field', async () => {
    await renderPage();
    expect(screen.getByLabelText('管理员邮箱')).toBeInTheDocument();
  });

  it('shows password input field with placeholder', async () => {
    await renderPage();
    const input = screen.getByLabelText('密码');
    expect(input).toBeInTheDocument();
    expect(input).toHaveAttribute('placeholder', '至少 6 位');
  });

  it('shows confirm password input field', async () => {
    await renderPage();
    expect(screen.getByLabelText('确认密码')).toBeInTheDocument();
  });

  it('shows "创建管理员" submit button', async () => {
    await renderPage();
    expect(screen.getByRole('button', { name: '创建管理员' })).toBeInTheDocument();
  });
});
