import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    listFeedback: vi.fn(),
    updateFeedback: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const item = {
  id: 'fb-1',
  userId: 'user-1',
  category: 'bug',
  body: '答题页崩了',
  route: '/learn',
  createdAt: '2026-05-28T10:00:00Z',
  priority: 'normal',
  status: 'open',
  assigneeAdminId: null,
  resolvedAt: null,
  resolution: null,
};

describe('FeedbackPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.listFeedback.mockResolvedValue({
      data: [item],
      total: 1,
      page: 1,
      perPage: 20,
      totalPages: 1,
    });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/FeedbackPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders hero title "用户反馈" and status filter chips', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('用户反馈')).toBeInTheDocument());
    // 5 个 filter chip:全部 / 待处理 / 处理中 / 已解决 / 已关闭
    expect(screen.getByRole('button', { name: '全部' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '待处理' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '已解决' })).toBeInTheDocument();
  });

  it('renders feedback row from list', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('答题页崩了')).toBeInTheDocument());
  });

  it('clicking filter chip calls listFeedback with status param', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('答题页崩了')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '待处理' }));
    await waitFor(() =>
      expect(mockApi.listFeedback).toHaveBeenLastCalledWith({
        page: 1,
        perPage: 20,
        status: 'open',
      }),
    );
  });

  it('clicking row opens drawer with 反馈详情', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('答题页崩了')).toBeInTheDocument());
    fireEvent.click(screen.getByText('答题页崩了'));
    await waitFor(() => expect(screen.getByText('反馈详情')).toBeInTheDocument());
  });

  it('saves status change via PATCH', async () => {
    mockApi.updateFeedback.mockResolvedValue({ ...item, status: 'resolved' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('答题页崩了')).toBeInTheDocument());
    fireEvent.click(screen.getByText('答题页崩了'));
    await waitFor(() => expect(screen.getByText('反馈详情')).toBeInTheDocument());
    // 直接点保存
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() =>
      expect(mockApi.updateFeedback).toHaveBeenCalledWith('fb-1', expect.objectContaining({
        priority: 'normal',
        status: 'open',
      })),
    );
  });
});
