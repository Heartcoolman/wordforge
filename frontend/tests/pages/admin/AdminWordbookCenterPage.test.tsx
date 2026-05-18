import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
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

const mockItem = {
  id: 'wb-1',
  name: 'CET-4 核心',
  description: '考试核心词',
  tags: ['考试', 'CET'],
  wordCount: 1500,
  version: '1.0',
  author: 'admin',
  imported: false,
  hasUpdate: false,
};
const mockItemImported = { ...mockItem, id: 'wb-2', name: 'TOEFL', imported: true, hasUpdate: true, tags: ['考试'] };

const mockPreview = {
  id: 'wb-1', name: 'CET-4 核心', description: '考试核心词', wordCount: 5, version: '1.0', author: 'admin',
  words: { total: 5, totalPages: 1, data: [{ spelling: 'apple', phonetic: '/ˈæpəl/', meanings: ['苹果'] }] },
};

describe('AdminWordbookCenterPage', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/admin/AdminWordbookCenterPage');
    return renderWithProviders(() => <Page />);
  }

  it('shows 未配置 hint when no wordbook center URL', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: '' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('尚未配置词书中心 URL')).toBeInTheDocument());
  });

  it('lists items when configured', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem, mockItemImported]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    expect(screen.getByText('TOEFL')).toBeInTheDocument();
    expect(screen.getByText('有更新')).toBeInTheDocument();
  });

  it('shows empty state when items empty', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无词书')).toBeInTheDocument());
  });

  it('checks for updates and shows banner', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterUpdates.mockResolvedValue([{ remoteId: 'wb-3', name: 'IELTS' }]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    fireEvent.click(screen.getByText('检查更新'));
    await waitFor(() => expect(screen.getByText(/1 本词书有更新/)).toBeInTheDocument());
  });

  it('shows toast when updates none', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterUpdates.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    fireEvent.click(screen.getByText('检查更新'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith('所有词书均为最新'));
  });

  it('handles updates check failure', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterUpdates.mockRejectedValue(new Error('net err'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    fireEvent.click(screen.getByText('检查更新'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('filters by search input', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem, { ...mockItem, id: 'wb-9', name: 'BEC', description: '商务' }]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    const search = screen.getByPlaceholderText('搜索词书...') as HTMLInputElement;
    fireEvent.input(search, { target: { value: 'BEC' } });
    await waitFor(() => expect(screen.queryByText('CET-4 核心')).not.toBeInTheDocument());
    expect(screen.getByText('BEC')).toBeInTheDocument();
  });

  it('filters by tag click', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem, { ...mockItem, id: 'wb-9', name: 'BEC', tags: ['商务'] }]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    const tagBtn = screen.getAllByText('考试').find((el) => el.tagName === 'BUTTON');
    if (tagBtn) {
      fireEvent.click(tagBtn);
      await waitFor(() => expect(screen.queryByText('BEC')).not.toBeInTheDocument());
      // 再点一次取消
      fireEvent.click(tagBtn);
    }
  });

  it('imports an item successfully', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterImport.mockResolvedValue({ wordbook: { name: 'CET-4 核心' }, wordsImported: 1500 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    const importBtn = screen.getByText('导入为系统词书');
    fireEvent.click(importBtn);
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('handles import failure', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterImport.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导入为系统词书'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('syncs an imported item with update', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItemImported]);
    mockApi.wbCenterSync.mockResolvedValue({ wordsAdded: 3, wordsUpdated: 2, wordsRemoved: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('TOEFL')).toBeInTheDocument());
    fireEvent.click(screen.getByText('同步更新'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('handles sync failure', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItemImported]);
    mockApi.wbCenterSync.mockRejectedValue(new Error('net'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('TOEFL')).toBeInTheDocument());
    fireEvent.click(screen.getByText('同步更新'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('opens preview modal on card click', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterPreview.mockResolvedValue(mockPreview);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    // 点击 Card 触发 handlePreview
    const card = screen.getByText('CET-4 核心').closest('div')!.parentElement!.parentElement!;
    fireEvent.click(card);
    await waitFor(() => expect(screen.getByText('apple')).toBeInTheDocument());
  });

  it('handles preview failure', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterPreview.mockRejectedValue(new Error('preview err'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4 核心')).toBeInTheDocument());
    const card = screen.getByText('CET-4 核心').closest('div')!.parentElement!.parentElement!;
    fireEvent.click(card);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('handles browse failure gracefully', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockRejectedValue(new Error('500'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('尚未配置词书中心 URL')).toBeInTheDocument());
  });
});
