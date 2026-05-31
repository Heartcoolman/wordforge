import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 本组聚焦底部「导入新词库」面板:远程目录浏览/导入/同步/检查更新/预览(保留全部远程导入能力)。
vi.mock('@/api/admin', () => ({
  adminApi: {
    adminWordbooksList: vi.fn(),
    adminWordbookStats: vi.fn(),
    adminWordbookWords: vi.fn(),
    adminWordbookHeatmap: vi.fn(),
    adminWordbookDistribution: vi.fn(),
    adminWordbookHistory: vi.fn(),
    getSettings: vi.fn(),
    wbCenterBrowse: vi.fn(),
    wbCenterUpdates: vi.fn(),
    wbCenterImport: vi.fn(),
    wbCenterSync: vi.fn(),
    wbCenterPreview: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const remoteItem = {
  id: 'rm-1', name: 'CET-4', description: 'desc', tags: ['考试'],
  wordCount: 100, version: '1.0', author: 'a', imported: false, hasUpdate: false,
};
const remoteImported = {
  id: 'rm-imp', name: 'TOEFL', description: '', tags: ['t'], wordCount: 50,
  version: '1.0', author: 'a', imported: true, hasUpdate: true, localWordbookId: 'wb-toefl',
};
const remotePreview = {
  id: 'rm-1', name: 'CET-4', description: 'desc', wordCount: 3, version: '1.0', author: 'a',
  words: { total: 3, page: 1, perPage: 20, totalPages: 1, data: [
    { spelling: 'apple', phonetic: '/ˈæpəl/', meanings: ['苹果'], examples: [] },
  ] },
};

// 左侧本地列表给空,避免与远程面板断言冲突
function stubLocalEmpty() {
  mockApi.adminWordbooksList.mockResolvedValue({
    items: [], total: 0, page: 1, perPage: 200, totalPages: 0,
    counts: { all: 0, system: 0, user: 0, totalEntries: 0 },
  });
}

async function renderPage() {
  const { default: Page } = await import('@/pages/WordbookCenterPage');
  return renderWithProviders(() => <Page />);
}

describe('WordbookCenterPage — 底部远程导入面板', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stubLocalEmpty();
  });

  it('未配置词书中心 URL 时显示提示', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: '' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('尚未配置词书中心 URL')).toBeInTheDocument());
  });

  it('已配置时列出远程词书 + 已导入/有更新角标', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem, remoteImported]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('TOEFL')).toBeInTheDocument());
    expect(screen.getByText('CET-4')).toBeInTheDocument();
    expect(screen.getByText('有更新')).toBeInTheDocument();
  });

  it('远程目录为空时显示空态', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('远程目录为空')).toBeInTheDocument());
  });

  it('检查更新后弹横幅', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem]);
    mockApi.wbCenterUpdates.mockResolvedValue([{ remoteId: 'rm-9', name: 'IELTS' }]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('检查更新'));
    await waitFor(() => expect(screen.getByText(/1 本词书有更新/)).toBeInTheDocument());
  });

  it('检查更新无结果时提示已最新', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem]);
    mockApi.wbCenterUpdates.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('检查更新'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('所有词书均为最新'));
  });

  it('导入远程词书:确认后调用 import', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem]);
    mockApi.wbCenterImport.mockResolvedValue({ wordbook: { name: 'CET-4' }, wordsImported: 100 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导入'));
    await waitFor(() => expect(screen.getByText('确认导入词库')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认导入'));
    await waitFor(() => expect(mockApi.wbCenterImport).toHaveBeenCalledWith('rm-1'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('同步有更新的远程词书', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteImported]);
    mockApi.wbCenterSync.mockResolvedValue({ wordsAdded: 3, wordsUpdated: 2, wordsRemoved: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('TOEFL')).toBeInTheDocument());
    fireEvent.click(screen.getByText('同步更新'));
    await waitFor(() => expect(mockApi.wbCenterSync).toHaveBeenCalledWith('rm-imp'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('预览远程词书', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem]);
    mockApi.wbCenterPreview.mockResolvedValue(remotePreview);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('预览'));
    await waitFor(() => expect(screen.getByText('apple')).toBeInTheDocument());
  });

  it('远程目录加载失败时弹错误但仍视为已配置', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockRejectedValue(new Error('500'));
    await renderPage();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('加载远程词库目录失败', '500'));
  });
});
