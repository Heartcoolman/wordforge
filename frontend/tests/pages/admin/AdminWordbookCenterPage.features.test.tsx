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
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockItem = {
  id: 'wb-1', name: 'CET-4', description: 'desc', tags: ['考试'],
  wordCount: 100, version: '1.0', author: 'a', imported: false, hasUpdate: false,
};
const importedNoUpdate = {
  id: 'wb-imp', name: 'Imported', description: '', tags: ['t'], wordCount: 50,
  version: '1.0', author: 'a', imported: true, hasUpdate: false,
};
const mockPreview = {
  id: 'wb-1', name: 'CET-4', description: 'desc', wordCount: 3, version: '1.0', author: 'a',
  words: { total: 3, totalPages: 2, data: [
    { spelling: 'apple', phonetic: '/ˈæpəl/', meanings: ['苹果'] },
    { spelling: 'banana', phonetic: '', meanings: [] },
    { spelling: 'cherry', phonetic: '', meanings: ['樱桃'] },
  ] },
};

async function renderPage() {
  const { default: Page } = await import('@/pages/admin/AdminWordbookCenterPage');
  return renderWithProviders(() => <Page />);
}

describe('AdminWordbookCenterPage — sync, preview, imported badge', () => {
  beforeEach(() => vi.clearAllMocks());

  it('clicks 同步「...」 button in updates banner', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterUpdates.mockResolvedValue([{ remoteId: 'wb-3', name: 'IELTS' }]);
    mockApi.wbCenterSync.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('检查更新'));
    await waitFor(() => expect(screen.getByText(/1 本词书有更新/)).toBeInTheDocument());
    fireEvent.click(screen.getByText('同步「IELTS」'));
    await waitFor(() => expect(mockApi.wbCenterSync).toHaveBeenCalled());
  });

  it('opens preview modal then closes it', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([mockItem]);
    mockApi.wbCenterPreview.mockResolvedValue(mockPreview);
    await renderPage();
    await waitFor(() => expect(screen.getByText('CET-4')).toBeInTheDocument());
    fireEvent.click(screen.getByText('CET-4').closest('[class*="cursor-pointer"]') as HTMLElement);
    await waitFor(() => expect(screen.getByText('apple')).toBeInTheDocument());
    expect(screen.getByText(/显示前.*\/.*词/)).toBeInTheDocument();
    const closeBtn = document.querySelector('[aria-label="关闭"]') as HTMLButtonElement;
    fireEvent.click(closeBtn);
    await waitFor(() => expect(screen.queryByText('apple')).not.toBeInTheDocument());
  });

  it('shows 已导入 badge when imported=true & hasUpdate=false', async () => {
    mockApi.getSettings.mockResolvedValue({ wordbookCenterUrl: 'https://x.com' });
    mockApi.wbCenterBrowse.mockResolvedValue([importedNoUpdate]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('已导入')).toBeInTheDocument());
  });
});
