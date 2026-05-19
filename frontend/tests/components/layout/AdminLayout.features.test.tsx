import { describe, it, expect, vi } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/lib/token', () => ({
  tokenManager: { clearAdminToken: vi.fn(), getAdminToken: vi.fn(() => 'tok') },
}));
vi.mock('@/api/admin', () => ({
  adminApi: {
    logout: vi.fn(),
    verifyToken: vi.fn().mockResolvedValue({ id: '1', email: 'admin@x.com' }),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('AdminLayout extra', () => {
  it('renders admin email after verify', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    await waitFor(() => expect(screen.getByText('admin@x.com')).toBeInTheDocument());
  });

  it('toggles sidebar collapse', async () => {
    // 折叠按钮按 matchMedia('(min-width: 768px)') 切换 collapsed/mobileOpen；
    // setup.ts 默认 mock 所有 query 返回 false，会让按钮误判为移动端走关抽屉分支。
    // 这里把视口断点覆盖为桌面态。
    const originalMatchMedia = window.matchMedia;
    window.matchMedia = vi.fn((query: string) => ({
      matches: query.includes('min-width: 768px'),
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })) as unknown as typeof window.matchMedia;
    try {
      const { AdminLayout } = await import('@/components/layout/AdminLayout');
      renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
      const collapseBtn = screen.getByLabelText('折叠侧边栏');
      fireEvent.click(collapseBtn);
      await waitFor(() => expect(screen.getByLabelText('展开侧边栏')).toBeInTheDocument());
    } finally {
      window.matchMedia = originalMatchMedia;
    }
  });

  it('shows toast warning when logout API throws', async () => {
    (adminApi.logout as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('500'));
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    fireEvent.click(screen.getByRole('button', { name: '退出登录' }));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
  });
});
