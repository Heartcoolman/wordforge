import { describe, it, expect, vi } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    clearAdminToken: vi.fn(),
    getAdminToken: vi.fn(() => null),
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
});
