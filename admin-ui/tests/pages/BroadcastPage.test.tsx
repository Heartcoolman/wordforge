import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    broadcast: vi.fn(),
    broadcastUpdate: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('BroadcastPage', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/BroadcastPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders hero title "系统广播"', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统广播')).toBeInTheDocument());
  });

  it('renders 3 tab buttons (系统消息 / 更新通知 / 维护通告)', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getAllByRole('tab').length).toBe(3);
    });
  });

  it('default tab is "系统消息" with title + content inputs', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByPlaceholderText('通知标题')).toBeInTheDocument();
      expect(screen.getByPlaceholderText('通知内容')).toBeInTheDocument();
    });
  });

  it('shows warning when title/content empty', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('发送广播')).toBeInTheDocument());
    fireEvent.click(screen.getByText('发送广播'));
    // 期望 toast.warning 被调用（mock 在 stores/ui）
    const { uiStore } = await import('@/stores/ui');
    await waitFor(() => expect(uiStore.toast.warning).toHaveBeenCalledWith('请填写标题和内容'));
  });

  it('sends broadcast successfully via confirm dialog', async () => {
    mockApi.broadcast.mockResolvedValue({ sent: 5 });
    await renderPage();
    await waitFor(() => expect(screen.getByPlaceholderText('通知标题')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('通知标题'), { target: { value: 'T' } });
    fireEvent.input(screen.getByPlaceholderText('通知内容'), { target: { value: 'M' } });
    fireEvent.click(screen.getByText('发送广播'));
    // dialog title 含 "确认发送系统消息"，按钮是 "确认发送"；用 role=button 精确匹配
    await waitFor(() => expect(screen.getByRole('button', { name: '确认发送' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认发送' }));
    await waitFor(() => expect(mockApi.broadcast).toHaveBeenCalledWith({ title: 'T', message: 'M' }));
  });
});
