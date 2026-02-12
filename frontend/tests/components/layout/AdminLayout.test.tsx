import { describe, it, expect, vi } from 'vitest';
import { screen } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../../helpers/render';

const logoutMock = vi.fn();

vi.mock('@/lib/token', () => ({
  tokenManager: {
    clearAdminToken: vi.fn(),
    getAdminToken: vi.fn(() => null),
  },
}));

vi.mock('@/api/admin', () => ({
  adminApi: {
    logout: (...args: unknown[]) => logoutMock(...args),
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

describe('AdminLayout', () => {
  it('renders sidebar links', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    expect(screen.getByText('用户管理')).toBeInTheDocument();
    expect(screen.getByText('AMAS 配置')).toBeInTheDocument();
  });

  it('renders header', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    expect(screen.getByText('管理后台')).toBeInTheDocument();
  });

  it('calls admin logout api when clicking exit', async () => {
    logoutMock.mockResolvedValueOnce({ loggedOut: true });
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);

    await userEvent.click(screen.getByRole('button', { name: '退出' }));
    expect(logoutMock).toHaveBeenCalledTimes(1);
  });
});
