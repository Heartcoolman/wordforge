import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    listBroadcasts: vi.fn(),
    broadcast: vi.fn(),
    broadcastPreview: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const FAKE_BOARD = {
  stats: { total: 12, totalSent: 32725, avgReadRate: 0.61, online: 1429 },
  broadcasts: [
    {
      id: 'b-001',
      title: 'v0.7.4 现已可用',
      message: '已为所有客户端写入版本通告。',
      author: 'ZS',
      sentCount: 2847,
      readCount: 1936,
      readRate: 0.68,
      createdAt: '2026-05-28T22:18:00Z',
    },
  ],
};

describe('BroadcastPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.listBroadcasts.mockResolvedValue(FAKE_BOARD);
    mockApi.broadcastPreview.mockResolvedValue({ matched: 2847, total: 2847 });
    mockApi.broadcast.mockResolvedValue({ sent: 2847, broadcastId: 'b-002' });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/BroadcastPage');
    return renderWithProviders(() => <Page />);
  }

  it('渲染标题「系统广播」', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('系统广播')).toBeInTheDocument());
  });

  it('渲染撰写表单（标题 / 正文 placeholder）', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByPlaceholderText('例：12 月 5 日 02:00–04:00 计划内维护')).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/支持纯文本与/)).toBeInTheDocument();
    });
  });

  it('字数计数随输入更新', async () => {
    await renderPage();
    const titleInput = await screen.findByPlaceholderText('例：12 月 5 日 02:00–04:00 计划内维护');
    expect(screen.getByText('0 / 200')).toBeInTheDocument();
    fireEvent.input(titleInput, { target: { value: 'hello' } });
    await waitFor(() => expect(screen.getByText('5 / 200')).toBeInTheDocument());
  });

  it('标题/正文为空时发送按钮 disabled', async () => {
    await renderPage();
    const sendBtn = await screen.findByRole('button', { name: /发送广播/ });
    expect(sendBtn).toBeDisabled();
  });

  it('填写后点发送 → 开确认弹窗 → 确认 → broadcast 被以 {title,message} 调用', async () => {
    await renderPage();
    const titleInput = await screen.findByPlaceholderText('例：12 月 5 日 02:00–04:00 计划内维护');
    const msgInput = screen.getByPlaceholderText(/支持纯文本与/);
    fireEvent.input(titleInput, { target: { value: 'T' } });
    fireEvent.input(msgInput, { target: { value: 'M' } });

    const sendBtn = screen.getByRole('button', { name: /发送广播/ });
    await waitFor(() => expect(sendBtn).not.toBeDisabled());
    fireEvent.click(sendBtn);

    await waitFor(() => expect(screen.getByRole('button', { name: /立即发送/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /立即发送/ }));

    await waitFor(() => expect(mockApi.broadcast).toHaveBeenCalledWith({ title: 'T', message: 'M' }));
  });

  it('历史行与统计数从 mock 渲染', async () => {
    await renderPage();
    await waitFor(() => {
      // 历史行标题
      expect(screen.getByText('v0.7.4 现已可用')).toBeInTheDocument();
      // 统计：广播总数 12 / 平均阅读率 61% / 当前在线 1,429
      expect(screen.getByText('12')).toBeInTheDocument();
      expect(screen.getByText('61')).toBeInTheDocument();
      expect(screen.getByText('1,429')).toBeInTheDocument();
    });
  });

  it('点快捷模板填充表单', async () => {
    await renderPage();
    const tpl = await screen.findByText('计划内维护通知');
    fireEvent.click(tpl);
    const titleInput = screen.getByPlaceholderText('例：12 月 5 日 02:00–04:00 计划内维护') as HTMLInputElement;
    await waitFor(() => expect(titleInput.value).toContain('计划内维护'));
  });
});
