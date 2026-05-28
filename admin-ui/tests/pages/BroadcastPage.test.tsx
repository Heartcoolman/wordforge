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
    // m027:audience 字段未填 → undefined 表示后端走全员路径
    await waitFor(() => expect(mockApi.broadcast).toHaveBeenCalledWith({ title: 'T', message: 'M', audience: undefined }));
  });

  it('m027: 受众过滤 UI 显示 + 提交时塞 audience 字段', async () => {
    mockApi.broadcast.mockResolvedValue({ sent: 2 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('受众过滤')).toBeInTheDocument());

    // 初始 chip 显示"全员"
    expect(screen.getByText('全员')).toBeInTheDocument();

    // 点 web + ios 平台,填 versionMin / lastActiveDays
    fireEvent.input(screen.getByPlaceholderText('通知标题'), { target: { value: 'T2' } });
    fireEvent.input(screen.getByPlaceholderText('通知内容'), { target: { value: 'M2' } });
    fireEvent.click(screen.getByText('web'));
    fireEvent.click(screen.getByText('ios'));
    fireEvent.input(screen.getByPlaceholderText('例如 v0.7.0'), { target: { value: 'v0.7.0' } });
    fireEvent.input(screen.getByPlaceholderText('例如 7'), { target: { value: '7' } });

    fireEvent.click(screen.getByText('发送广播'));
    await waitFor(() => expect(screen.getByRole('button', { name: '确认发送' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认发送' }));

    await waitFor(() =>
      expect(mockApi.broadcast).toHaveBeenCalledWith({
        title: 'T2',
        message: 'M2',
        audience: {
          platforms: ['web', 'ios'],
          versionMin: 'v0.7.0',
          lastActiveDays: 7,
          userIds: undefined,
        },
      }),
    );
  });
});
