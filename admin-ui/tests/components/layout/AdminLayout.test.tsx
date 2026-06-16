import { describe, it, expect, vi } from 'vitest';
import { screen } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../../helpers/render';

const logoutMock = vi.fn();
const verifyTokenMock = vi.fn();
const healthMock = vi.fn();

vi.mock('@/lib/token', () => ({
  tokenManager: {
    clearAdminToken: vi.fn(),
    getAdminToken: vi.fn(() => null),
  },
}));

vi.mock('@/api/admin', () => ({
  adminApi: {
    logout: (...args: unknown[]) => logoutMock(...args),
    verifyToken: (...args: unknown[]) => verifyTokenMock(...args),
    health: (...args: unknown[]) => healthMock(...args),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: {
    toast: {
      success: vi.fn(),
      error: vi.fn(),
      warning: vi.fn(),
      info: vi.fn(),
    },
  },
}));

beforeEach(() => {
  logoutMock.mockReset().mockResolvedValue({ loggedOut: true });
  verifyTokenMock.mockReset().mockResolvedValue({ id: 'a1', email: 'admin@wf.test' });
  healthMock.mockReset().mockResolvedValue({ version: '1.2.0-beta.1' });
});

describe('AdminLayout', () => {
  it('renders sidebar links', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    // 重设计后的 7 组导航 —— 抽样断言关键链接仍在侧栏
    expect(screen.getByText('用户管理')).toBeInTheDocument();
    // PR redesign: '配置' → '调参'（对齐设计图）
    expect(screen.getByText('AMAS 调参')).toBeInTheDocument();
    // PR redesign: '系统广播' → '消息推送'
    expect(screen.getByText('消息推送')).toBeInTheDocument();
    expect(screen.getByText('资源包')).toBeInTheDocument();
  });

  it('renders header', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    // 默认路由（"/"）无匹配 nav item → 标题回退为「管理后台」
    expect(screen.getByRole('heading', { name: '管理后台' })).toBeInTheDocument();
  });

  it('calls admin logout api when clicking exit', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);

    // 重设计后退出登录为图标按钮，仍带 aria-label="退出登录"
    await userEvent.click(screen.getByRole('button', { name: '退出登录' }));
    expect(logoutMock).toHaveBeenCalledTimes(1);
  });
});
