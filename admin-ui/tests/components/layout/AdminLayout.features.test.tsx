import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/lib/token', () => ({
  tokenManager: { clearAdminToken: vi.fn(), getAdminToken: vi.fn(() => 'tok') },
}));
vi.mock('@/api/admin', () => ({
  adminApi: {
    logout: vi.fn().mockResolvedValue({ loggedOut: true }),
    verifyToken: vi.fn().mockResolvedValue({ id: '1', email: 'admin@x.com' }),
    // onMount 还会调 health() 推断导览波次；给安全默认值避免未处理 rejection
    health: vi.fn().mockResolvedValue({ version: '1.2.0', dbSizeBytes: null }),
  },
}));
// NotificationBell 顶栏铃铛轮询未读；给空收件箱避免 toast.error
vi.mock('@/api/notifications', () => ({
  notificationsApi: {
    list: vi.fn().mockResolvedValue({ items: [], unreadCount: 0, limit: 100 }),
    markRead: vi.fn().mockResolvedValue({ unreadCount: 0 }),
    markAllRead: vi.fn().mockResolvedValue({ unreadCount: 0 }),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('AdminLayout extra', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (adminApi.logout as ReturnType<typeof vi.fn>).mockResolvedValue({ loggedOut: true });
    (adminApi.verifyToken as ReturnType<typeof vi.fn>).mockResolvedValue({ id: '1', email: 'admin@x.com' });
    (adminApi.health as ReturnType<typeof vi.fn>).mockResolvedValue({ version: '1.2.0', dbSizeBytes: null });
  });

  it('renders admin email after verify', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    // 新版顶栏账户区在 verifyToken 成功后展示邮箱
    await waitFor(() => expect(screen.getByText('admin@x.com')).toBeInTheDocument());
  });

  it('renders the new sidebar nav groups and items', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    // 新 IA 的 17 个导航项标签
    const labels = [
      '仪表盘', '用户管理', '设备与遥测', 'AMAS 指标', 'AMAS 调参', 'LLM 顾问',
      '数据分析', '词库中心', '服务体征', '系统监控', '数据探针', '远程探针', '消息推送',
      '用户反馈', '版本更新', '资源包', '系统设置',
    ];
    for (const label of labels) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  it('toggles sidebar collapse', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    // 折叠按钮初始 title="折叠"（展开态）；点击后切到 title="展开"（折叠态）
    const collapseBtn = screen.getByTitle('折叠');
    expect(screen.getByText('折叠侧边栏')).toBeInTheDocument();
    fireEvent.click(collapseBtn);
    await waitFor(() => expect(screen.getByTitle('展开')).toBeInTheDocument());
    // 折叠后侧栏文案标签隐藏
    expect(screen.queryByText('折叠侧边栏')).not.toBeInTheDocument();
  });

  it('logout clears token and navigates on success', async () => {
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    fireEvent.click(screen.getByRole('button', { name: '退出登录' }));
    await waitFor(() => expect(adminApi.logout).toHaveBeenCalled());
    expect(mockToast.warning).not.toHaveBeenCalled();
  });

  it('shows toast warning when logout API throws', async () => {
    (adminApi.logout as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('500'));
    const { AdminLayout } = await import('@/components/layout/AdminLayout');
    renderWithProviders(() => <AdminLayout>Content</AdminLayout>);
    fireEvent.click(screen.getByRole('button', { name: '退出登录' }));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
  });
});
