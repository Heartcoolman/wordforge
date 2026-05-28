import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    setup: vi.fn(),
    checkStatus: vi.fn().mockResolvedValue({ initialized: false }),
  },
}));

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getAdminToken: vi.fn(() => null),
    setAdminToken: vi.fn(),
    clearAdminToken: vi.fn(),
    getToken: vi.fn(() => null),
    isAuthenticated: vi.fn(() => false),
    needsRefresh: vi.fn(() => false),
    refreshAccessToken: vi.fn().mockResolvedValue(false),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('SetupPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: SetupPage } = await import('@/pages/SetupPage');
    return renderWithProviders(() => <SetupPage />);
  }

  /**
   * PR redesign: SetupPage 现在是 3 步 wizard，初始 step=1（环境检测）。
   * 检测通过后必须点"下一步：创建超管"才能进入表单（step 2）。
   * 这个 helper 把页面推进到 step 2。
   */
  async function gotoFormStep() {
    await waitFor(() => {
      // step 1 自动检测完成后显示这个按钮
      expect(screen.getByRole('button', { name: /下一步：创建超管/ })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: /下一步：创建超管/ }));
    await waitFor(() => expect(screen.getByLabelText('管理员邮箱')).toBeInTheDocument());
  }

  it('shows "初始化管理后台" heading', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('初始化管理后台')).toBeInTheDocument();
    });
  });

  it('shows wizard progress chips for 3 steps', async () => {
    await renderPage();
    await waitFor(() => {
      // 3 步 chip 文案
      expect(screen.getByText('环境检测')).toBeInTheDocument();
      expect(screen.getByText('创建超管')).toBeInTheDocument();
      expect(screen.getByText('登录')).toBeInTheDocument();
    });
  });

  it('shows email input field after entering step 2', async () => {
    await renderPage();
    await gotoFormStep();
    expect(screen.getByLabelText('管理员邮箱')).toBeInTheDocument();
  });

  it('shows password input field with placeholder after entering step 2', async () => {
    await renderPage();
    await gotoFormStep();
    const input = screen.getByLabelText('密码');
    expect(input).toBeInTheDocument();
    expect(input).toHaveAttribute('placeholder', '至少 8 位');
  });

  it('shows confirm password input field after entering step 2', async () => {
    await renderPage();
    await gotoFormStep();
    expect(screen.getByLabelText('确认密码')).toBeInTheDocument();
  });

  it('shows "创建管理员" submit button after entering step 2', async () => {
    await renderPage();
    await gotoFormStep();
    expect(screen.getByRole('button', { name: '创建管理员' })).toBeInTheDocument();
  });
});
