import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent, within } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 本组聚焦顶部「远程目录」面板:URL 配置门控 / 远程目录浏览 / 导入 / 同步 / 预览 / 可同步更新列表。
// 重设计后远程面板由右上「远程目录」按钮切换显示,且更新检查改为自动加载到「可同步更新」面板。
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
  id: 'rm-1', name: 'CET-4', description: 'desc', tags: ['考试'], localTags: [],
  wordCount: 100, version: '1.0', author: 'a', imported: false, hasUpdate: false,
};
const remoteImported = {
  id: 'rm-imp', name: 'TOEFL', description: '', tags: ['t'], localTags: [], wordCount: 50,
  version: '2.0', author: 'a', imported: true, hasUpdate: true,
  localWordbookId: 'wb-toefl', localVersion: '1.0',
};
const remotePreview = {
  id: 'rm-1', name: 'CET-4', description: 'desc', wordCount: 3, version: '1.0', author: 'a',
  tags: ['考试'],
  words: {
    total: 3, page: 1, perPage: 20, totalPages: 1, data: [
      { spelling: 'apple', phonetic: '/ˈæpəl/', meanings: ['苹果'], examples: [] },
    ],
  },
};
const updateInfo = {
  remoteId: 'rm-imp', name: 'TOEFL', localVersion: '1.0', remoteVersion: '2.0', localWordbookId: 'wb-toefl',
};

// 左侧本地列表给空,避免与远程面板断言冲突
function stubLocalEmpty() {
  mockApi.adminWordbooksList.mockResolvedValue({
    items: [], total: 0, page: 1, perPage: 200, totalPages: 0,
    counts: { all: 0, system: 0, user: 0, totalEntries: 0 },
  });
}

// 渲染并展开顶部远程目录面板(重设计后默认折叠,需点击「远程目录」按钮)。
async function renderPageWithRemote() {
  const { default: Page } = await import('@/pages/WordbookCenterPage');
  const utils = renderWithProviders(() => <Page />);
  // 等待本地列表加载完成后再展开,确保资源初始化稳定
  await waitFor(() => expect(screen.getByText('全部词库')).toBeInTheDocument());
  fireEvent.click(screen.getByText('远程目录'));
  return utils;
}

describe('WordbookCenterPage — 远程目录面板', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    stubLocalEmpty();
    // 所有远程接口给安全默认值,避免未处理的 rejection
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: '' });
    mockApi.wbCenterBrowse.mockResolvedValue([]);
    mockApi.wbCenterUpdates.mockResolvedValue([]);
    mockApi.wbCenterImport.mockResolvedValue({
      wordbook: { id: 'wb-x', name: 'CET-4', description: '', type: 'system', wordCount: 100 },
      wordsImported: 100, wordsSkipped: 0,
    });
    mockApi.wbCenterSync.mockResolvedValue({
      wordbook: { id: 'wb-toefl', name: 'TOEFL', wordCount: 53 },
      wordsAdded: 3, wordsUpdated: 2, wordsRemoved: 1,
    });
    mockApi.wbCenterPreview.mockResolvedValue(remotePreview);
  });

  it('未配置词书中心 URL 时显示提示', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: '' });
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('尚未配置词书中心 URL')).toBeInTheDocument());
  });

  it('已配置时列出远程词书 + 已导入/有更新角标', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem, remoteImported]);
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('TOEFL')).toBeInTheDocument());
    expect(screen.getByText('CET-4')).toBeInTheDocument();
    expect(screen.getByText('有更新')).toBeInTheDocument();
  });

  it('远程目录为空时显示空态', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([]);
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('远程目录为空')).toBeInTheDocument());
  });

  it('有可同步更新时在「可同步更新」面板列出条目', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteImported]);
    mockApi.wbCenterUpdates.mockResolvedValue([updateInfo]);
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('可同步更新')).toBeInTheDocument());
    // 更新面板渲染本地→远端版本号
    await waitFor(() => expect(screen.getByText('v1.0 → v2.0')).toBeInTheDocument());
  });

  it('无可同步更新时提示均为最新', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem]);
    mockApi.wbCenterUpdates.mockResolvedValue([]);
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('均为最新')).toBeInTheDocument());
  });

  it('导入远程词书:确认后调用 import', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem]);
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导入'));
    // 弹出确认弹窗
    await waitFor(() => expect(screen.getByText('确认导入词库')).toBeInTheDocument());
    // 弹窗页脚的「导入」确认按钮(与卡片按钮同文案,取页脚内的那个)
    const dialog = screen.getByText('确认导入词库').closest('.card') as HTMLElement;
    fireEvent.click(within(dialog).getByRole('button', { name: '导入' }));
    await waitFor(() => expect(mockApi.wbCenterImport).toHaveBeenCalledWith('rm-1'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('同步有更新的远程词书', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteImported]);
    mockApi.wbCenterUpdates.mockResolvedValue([]);
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('TOEFL')).toBeInTheDocument());
    // 卡片上「同步」按钮(已导入且有更新)
    fireEvent.click(screen.getByText('同步'));
    await waitFor(() => expect(mockApi.wbCenterSync).toHaveBeenCalledWith('rm-imp'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('预览远程词书', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([remoteItem]);
    mockApi.wbCenterPreview.mockResolvedValue(remotePreview);
    await renderPageWithRemote();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('预览'));
    await waitFor(() => expect(screen.getByText('apple')).toBeInTheDocument());
    expect(mockApi.wbCenterPreview).toHaveBeenCalledWith('rm-1', { perPage: 20 });
  });

  it('远程目录加载失败时弹错误并回退到未配置态', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockRejectedValue(new Error('500'));
    await renderPageWithRemote();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('加载远程词库目录失败', '500'));
  });
});
