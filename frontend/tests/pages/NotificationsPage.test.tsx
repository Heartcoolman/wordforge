import { describe, it, expect, vi, beforeEach } from 'vitest';
import { waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// Mock dependencies
vi.mock('@/api/notifications', () => ({
  notificationsApi: {
    list: vi.fn(),
    markRead: vi.fn(),
    markAllRead: vi.fn(),
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

vi.mock('@/utils/formatters', () => ({
  formatRelativeTime: vi.fn((iso: string) => '刚刚'),
}));

import { notificationsApi } from '@/api/notifications';
import type { Notification } from '@/types/notification';

const mockApi = notificationsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

function createFakeNotification(overrides?: Partial<Notification>): Notification {
  return {
    id: 'notif-1',
    userId: 'user-1',
    title: '测试通知标题',
    message: '测试通知消息内容',
    type: 'info',
    read: false,
    createdAt: new Date().toISOString(),
    ...overrides,
  };
}

describe('NotificationsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows "通知" heading', async () => {
    mockApi.list.mockResolvedValue([]);
    const { default: NotificationsPage } = await import('@/pages/NotificationsPage');
    const { getByText } = renderWithProviders(() => <NotificationsPage />);
    expect(getByText('通知')).toBeInTheDocument();
  });

  it('shows loading spinner initially', async () => {
    // Never resolve so loading stays true
    mockApi.list.mockReturnValue(new Promise(() => {}));
    const { default: NotificationsPage } = await import('@/pages/NotificationsPage');
    const { container } = renderWithProviders(() => <NotificationsPage />);
    // Spinner renders an SVG or element with role
    const spinner = container.querySelector('[class*="animate-spin"], [role="status"]');
    expect(spinner).toBeTruthy();
  });

  it('shows empty state "暂无通知" when no notifications', async () => {
    mockApi.list.mockResolvedValue([]);
    const { default: NotificationsPage } = await import('@/pages/NotificationsPage');
    const { findByText } = renderWithProviders(() => <NotificationsPage />);
    expect(await findByText('暂无通知')).toBeInTheDocument();
  });

  it('shows notification list with title and message', async () => {
    const notifications: Notification[] = [
      createFakeNotification({ id: 'n1', title: '系统更新', message: '版本已更新至1.2', read: true }),
      createFakeNotification({ id: 'n2', title: '学习提醒', message: '今天还没有学习哦', read: true }),
    ];
    mockApi.list.mockResolvedValue(notifications);
    const { default: NotificationsPage } = await import('@/pages/NotificationsPage');
    const { findByText } = renderWithProviders(() => <NotificationsPage />);

    expect(await findByText('系统更新')).toBeInTheDocument();
    expect(await findByText('版本已更新至1.2')).toBeInTheDocument();
    expect(await findByText('学习提醒')).toBeInTheDocument();
    expect(await findByText('今天还没有学习哦')).toBeInTheDocument();
  });

  it('shows unread badge and "全部已读" button when unread items exist', async () => {
    const notifications: Notification[] = [
      createFakeNotification({ id: 'n1', read: false }),
      createFakeNotification({ id: 'n2', read: false }),
      createFakeNotification({ id: 'n3', read: true }),
    ];
    mockApi.list.mockResolvedValue(notifications);
    const { default: NotificationsPage } = await import('@/pages/NotificationsPage');
    const { findByText } = renderWithProviders(() => <NotificationsPage />);

    expect(await findByText('2 未读')).toBeInTheDocument();
    expect(await findByText('全部已读')).toBeInTheDocument();
  });

  it('does not show unread badge or "全部已读" when all are read', async () => {
    const notifications: Notification[] = [
      createFakeNotification({ id: 'n1', title: '已读通知一', read: true }),
      createFakeNotification({ id: 'n2', title: '已读通知二', read: true }),
    ];
    mockApi.list.mockResolvedValue(notifications);
    const { default: NotificationsPage } = await import('@/pages/NotificationsPage');
    const { findByText, queryByText } = renderWithProviders(() => <NotificationsPage />);

    // Wait for data to load
    await findByText('已读通知一');
    expect(queryByText('未读')).not.toBeInTheDocument();
    expect(queryByText('全部已读')).not.toBeInTheDocument();
  });
});
