import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../helpers/render';

// 重设计后的 BroadcastPage 是统一推送中心，除历史/统计外还调用排程队列、
// 草稿存取与受众预估等 adminApi 方法。每个 mock 都给安全默认值，避免 unhandled rejection。
vi.mock('@/api/admin', () => ({
  adminApi: {
    listBroadcasts: vi.fn(),
    listScheduledBroadcasts: vi.fn(),
    broadcast: vi.fn(),
    broadcastPreview: vi.fn(),
    getPushDraft: vi.fn(),
    savePushDraft: vi.fn(),
    deletePushDraft: vi.fn(),
    cancelScheduledBroadcast: vi.fn(),
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
  pagination: { total: 1, offset: 0, limit: 50 },
};

describe('BroadcastPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.listBroadcasts.mockResolvedValue(FAKE_BOARD);
    mockApi.listScheduledBroadcasts.mockResolvedValue({ items: [] });
    mockApi.broadcastPreview.mockResolvedValue({ matched: 2847, total: 2847 });
    mockApi.broadcast.mockResolvedValue({ sent: 2847, scheduled: false, broadcastId: 'b-002' });
    mockApi.getPushDraft.mockResolvedValue({ draft: null });
    mockApi.savePushDraft.mockResolvedValue({ draft: { title: '', message: '' } });
    mockApi.deletePushDraft.mockResolvedValue({ deleted: true });
    mockApi.cancelScheduledBroadcast.mockResolvedValue({ canceled: true });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/BroadcastPage');
    return renderWithProviders(() => <Page />);
  }

  it('渲染标题「消息推送」', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('消息推送')).toBeInTheDocument());
  });

  it('渲染撰写表单（标题 / 正文 placeholder）', async () => {
    await renderPage();
    await waitFor(() => {
      expect(screen.getByPlaceholderText('如：v1.4.3 更新公告')).toBeInTheDocument();
      expect(screen.getByPlaceholderText('支持纯文本…')).toBeInTheDocument();
    });
  });

  it('标题输入实时反映到右侧预览', async () => {
    await renderPage();
    const titleInput = await screen.findByPlaceholderText('如：v1.4.3 更新公告');
    // 初始预览占位文案
    expect(screen.getByText('广播标题')).toBeInTheDocument();
    fireEvent.input(titleInput, { target: { value: '维护通告' } });
    await waitFor(() => expect(screen.getByText('维护通告')).toBeInTheDocument());
  });

  it('标题/正文为空时发送按钮 disabled', async () => {
    await renderPage();
    const sendBtn = await screen.findByRole('button', { name: /立即发送/ });
    expect(sendBtn).toBeDisabled();
  });

  it('填写后点发送 → 开确认弹窗 → 确认 → broadcast 被以 {title,message} 调用', async () => {
    await renderPage();
    const titleInput = await screen.findByPlaceholderText('如：v1.4.3 更新公告');
    const msgInput = screen.getByPlaceholderText('支持纯文本…');
    fireEvent.input(titleInput, { target: { value: 'T' } });
    fireEvent.input(msgInput, { target: { value: 'M' } });

    const sendBtn = screen.getByRole('button', { name: /立即发送/ });
    await waitFor(() => expect(sendBtn).not.toBeDisabled());
    fireEvent.click(sendBtn);

    // 确认弹窗：标题「确认发送广播？」+ 确认按钮「确认发送」
    await waitFor(() => expect(screen.getByText('确认发送广播？')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /确认发送/ }));

    await waitFor(() =>
      expect(mockApi.broadcast).toHaveBeenCalledWith(expect.objectContaining({ title: 'T', message: 'M' })),
    );
  });

  it('统计卡从 mock 渲染（广播总数 / 平均阅读率）', async () => {
    await renderPage();
    await waitFor(() => {
      // 广播总数 12
      expect(screen.getByText('12')).toBeInTheDocument();
      // 平均阅读率 61（avgReadRate 0.61 → 61，单位 % 独立渲染）
      expect(screen.getByText('61')).toBeInTheDocument();
    });
  });

  it('切到历史标签渲染历史行', async () => {
    const user = userEvent.setup();
    await renderPage();
    await waitFor(() => expect(screen.getByText('消息推送')).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: '历史' }));
    await waitFor(() => expect(screen.getByText('v0.7.4 现已可用')).toBeInTheDocument());
  });

  it('勾选平台受众过滤触发受众预估请求', async () => {
    await renderPage();
    // 等待初始受众预估（防抖 450ms 后）完成至少一次
    await waitFor(() => expect(mockApi.broadcastPreview).toHaveBeenCalled());
    const before = mockApi.broadcastPreview.mock.calls.length;

    // 平台按钮带 class="badge"，其 accessible name 在 happy-dom 下不稳定，直接取文本节点点击。
    const iosBtn = screen.getByText('ios');
    expect(iosBtn.closest('button')).not.toBeNull();
    fireEvent.click(iosBtn);

    await waitFor(() =>
      expect(mockApi.broadcastPreview.mock.calls.length).toBeGreaterThan(before),
    );
    // 受众条件含 ios 平台
    const lastArg = mockApi.broadcastPreview.mock.calls.at(-1)?.[0];
    expect(lastArg?.audience?.platforms).toContain('ios');
  });
});
