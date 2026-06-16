import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    listFeedback: vi.fn(),
    getFeedbackStats: vi.fn(),
    getFeedbackDetail: vi.fn(),
    createFeedbackReply: vi.fn(),
    assignFeedback: vi.fn(),
    resolveFeedback: vi.fn(),
    mergeFeedback: vi.fn(),
    createFeedbackGithubIssue: vi.fn(),
    markAllFeedbackRead: vi.fn(),
    feedbackCsvUrl: vi.fn(),
    getFeedbackDraft: vi.fn(),
    saveFeedbackDraft: vi.fn(),
    feedbackAnnouncements: vi.fn(),
    createFeedbackAnnouncement: vi.fn(),
    updateFeedbackAnnouncement: vi.fn(),
    deleteFeedbackAnnouncement: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const item = {
  id: 'fb-12345678',
  userId: 'user-1',
  userName: '王芮',
  platform: 'ios',
  appVersion: '0.7.3',
  category: 'bug',
  body: '点击推送进入闪退\n现象:锁屏点击通知后白屏闪退',
  route: '/learn',
  createdAt: new Date(Date.now() - 3 * 60 * 1000).toISOString(),
  priority: 'urgent',
  status: 'open',
  assigneeAdminId: null,
  resolvedAt: null,
  resolution: null,
  readAt: null,
  firstResponseAt: null,
  csatScore: null,
  csatComment: null,
  dedupCount: 2,
  githubIssueUrl: null,
  deviceProfile: { osName: 'iOS 17.3', network: '5G', push: 'APNS' },
};

const stats = {
  total: 421,
  unresolved: 14,
  byCategory: { bug: 5, feature: 4, complaint: 3, praise: 2 },
  byStatus: { open: 14, inProgress: 0, resolved: 397, closed: 10 },
  unreadCount: 12,
  assignedCount: 8,
  resolvedCount: 397,
  medianResponseMinutes: 14,
  resolveRate7d: 0.924,
  csatAvg: 4.6,
  csatCount: 287,
  responseSpark: [],
  resolveSpark: [],
  csatSpark: [],
  responseDelta: null,
  resolveDelta: null,
  csatDelta: null,
};

const detail = {
  item,
  replies: [
    {
      id: 'r1',
      feedbackId: 'fb-12345678',
      authorKind: 'admin',
      authorId: 'lu',
      body: '已认领',
      pushInapp: true,
      ccEmail: false,
      createdAt: item.createdAt,
    },
  ],
  events: [
    {
      id: 'e1',
      feedbackId: 'fb-12345678',
      kind: 'submitted',
      actor: '王芮',
      summary: '提交了 Bug 报告',
      refId: null,
      createdAt: item.createdAt,
    },
    {
      id: 'e2',
      feedbackId: 'fb-12345678',
      kind: 'reply',
      actor: '陆远',
      summary: '回复了用户',
      refId: 'r1',
      createdAt: item.createdAt,
    },
  ],
  attachments: [
    {
      id: 'a1',
      feedbackId: 'fb-12345678',
      name: 'screen-1.png',
      url: 'http://x/1.png',
      kind: 'image',
      createdAt: item.createdAt,
    },
  ],
};

describe('FeedbackPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.listFeedback.mockResolvedValue({ data: [item], total: 1, page: 1, perPage: 30, totalPages: 1 });
    mockApi.getFeedbackStats.mockResolvedValue(stats);
    mockApi.getFeedbackDetail.mockResolvedValue(detail);
    mockApi.markAllFeedbackRead.mockResolvedValue({ updated: 12 });
    mockApi.getFeedbackDraft.mockResolvedValue({ draft: null });
    mockApi.saveFeedbackDraft.mockResolvedValue({ id: 'd1', feedbackId: 'fb-12345678', body: '', pushInapp: true, ccEmail: false });
    mockApi.feedbackAnnouncements.mockResolvedValue({ data: [] });
    mockApi.createFeedbackReply.mockResolvedValue({ id: 'r2' });
    mockApi.resolveFeedback.mockResolvedValue(item);
    mockApi.assignFeedback.mockResolvedValue(item);
    mockApi.mergeFeedback.mockResolvedValue(item);
    mockApi.createFeedbackGithubIssue.mockResolvedValue({ issueUrl: 'http://gh/1' });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/FeedbackPage');
    return renderWithProviders(() => <Page />);
  }

  it('渲染页头标题与 KPI 统计', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('用户反馈')).toBeInTheDocument());
    // KPI 卡:已解决数(397 唯一)+ 已解决标签(Seg 与 KPI 卡同名,故多处)+ 中位响应(唯一)
    await waitFor(() => expect(screen.getByText('397')).toBeInTheDocument());
    expect(screen.getAllByText('已解决').length).toBeGreaterThan(0);
    expect(screen.getByText('中位响应')).toBeInTheDocument();
  });

  it('渲染列表行(主题取 body 首行)', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('点击推送进入闪退')).toBeInTheDocument());
  });

  it('选择分类 Bug 用 category 过滤', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('点击推送进入闪退')).toBeInTheDocument());
    const sel = document.querySelector('select.select') as HTMLSelectElement;
    expect(sel).toBeTruthy();
    fireEvent.change(sel, { target: { value: 'bug' } });
    await waitFor(() =>
      expect(mockApi.listFeedback).toHaveBeenLastCalledWith(
        expect.objectContaining({ category: 'bug' }),
      ),
    );
  });

  it('开启"仅未读"开关传 unread=true', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('点击推送进入闪退')).toBeInTheDocument());
    const unreadLabel = screen.getByText('仅未读').closest('label') as HTMLLabelElement;
    const cb = unreadLabel.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(cb).toBeTruthy();
    fireEvent.change(cb, { target: { checked: true } });
    await waitFor(() =>
      expect(mockApi.listFeedback).toHaveBeenLastCalledWith(
        expect.objectContaining({ unread: true }),
      ),
    );
  });

  it('点击行加载详情并渲染时间线', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('点击推送进入闪退')).toBeInTheDocument());
    fireEvent.click(screen.getByText('点击推送进入闪退'));
    await waitFor(() => expect(mockApi.getFeedbackDetail).toHaveBeenCalledWith('fb-12345678'));
    await waitFor(() => expect(screen.getByText('处理时间线')).toBeInTheDocument());
    // 事件 kind=submitted -> EVENT_LABEL「用户提交」
    expect(screen.getByText('用户提交')).toBeInTheDocument();
  });

  it('发送回复调用 createFeedbackReply', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('点击推送进入闪退')).toBeInTheDocument());
    fireEvent.click(screen.getByText('点击推送进入闪退'));
    await waitFor(() => expect(screen.getByText('处理时间线')).toBeInTheDocument());
    const ta = document.querySelector('textarea.textarea') as HTMLTextAreaElement;
    expect(ta).toBeTruthy();
    fireEvent.input(ta, { target: { value: '已修复' } });
    fireEvent.click(screen.getByRole('button', { name: '发送回复' }));
    await waitFor(() =>
      expect(mockApi.createFeedbackReply).toHaveBeenCalledWith(
        'fb-12345678',
        expect.objectContaining({ body: '已修复' }),
      ),
    );
  });

  it('全部已读经确认弹窗调用 markAllFeedbackRead', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('用户反馈')).toBeInTheDocument());
    // 页头按钮打开确认弹窗
    fireEvent.click(screen.getByRole('button', { name: '全部已读' }));
    await waitFor(() => expect(screen.getByText('全部标记已读？')).toBeInTheDocument());
    // 弹窗 footer 的确认按钮(同名「全部已读」,取最后一个)
    const btns = screen.getAllByRole('button', { name: '全部已读' });
    fireEvent.click(btns[btns.length - 1]);
    await waitFor(() => expect(mockApi.markAllFeedbackRead).toHaveBeenCalled());
  });

  it('选中为空时显示引导空态', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByText('点击推送进入闪退')).toBeInTheDocument());
    expect(screen.getByText('选择一条反馈查看详情')).toBeInTheDocument();
  });
});
