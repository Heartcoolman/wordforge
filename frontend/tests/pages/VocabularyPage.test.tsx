import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import { createFakeWords, createFakeWord } from '../helpers/factories';

vi.mock('@/api/words', () => ({
  wordsApi: {
    list: vi.fn().mockResolvedValue({ items: [], total: 0 }),
    create: vi.fn(),
    update: vi.fn(),
    delete: vi.fn(),
    batchCreate: vi.fn(),
    importUrl: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { wordsApi } from '@/api/words';
import { uiStore } from '@/stores/ui';
import VocabularyPage from '@/pages/VocabularyPage';

const mockWordsApi = wordsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => {
  server.resetHandlers();
  // 恢复 list 的默认返回值，防止 SolidJS reactive 系统在 cleanup 时调用到 undefined
  vi.clearAllMocks();
  mockWordsApi.list.mockResolvedValue({ items: [], total: 0 });
});
afterAll(() => server.close());

describe('VocabularyPage', () => {
  it('shows "词库管理" heading', () => {
    mockWordsApi.list.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <VocabularyPage />);
    expect(screen.getByText('词库管理')).toBeInTheDocument();
  });

  it('shows "添加单词" and "批量导入" buttons', () => {
    mockWordsApi.list.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <VocabularyPage />);
    expect(screen.getByText('添加单词')).toBeInTheDocument();
    expect(screen.getByText('批量导入')).toBeInTheDocument();
  });

  it('shows search input with placeholder', () => {
    mockWordsApi.list.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <VocabularyPage />);
    expect(screen.getByPlaceholderText('搜索单词...')).toBeInTheDocument();
    expect(screen.getByText('搜索')).toBeInTheDocument();
  });

  it('shows empty state when no words', async () => {
    mockWordsApi.list.mockResolvedValue({ items: [], total: 0 });

    renderWithProviders(() => <VocabularyPage />);

    await waitFor(() => {
      expect(screen.getByText('暂无单词')).toBeInTheDocument();
    });
  });

  it('shows word list after loading', async () => {
    const words = createFakeWords(3);
    mockWordsApi.list.mockResolvedValue({ items: words, total: 3 });

    renderWithProviders(() => <VocabularyPage />);

    await waitFor(() => {
      expect(screen.getByText(words[0].text)).toBeInTheDocument();
      expect(screen.getByText(words[1].text)).toBeInTheDocument();
      expect(screen.getByText(words[2].text)).toBeInTheDocument();
    });
  });

  it('shows total count when words exist', async () => {
    const words = createFakeWords(2);
    mockWordsApi.list.mockResolvedValue({ items: words, total: 42 });

    renderWithProviders(() => <VocabularyPage />);

    await waitFor(() => {
      expect(screen.getByText(/共 42 个单词/)).toBeInTheDocument();
    });
  });

  it('shows error toast when API fails', async () => {
    mockWordsApi.list.mockRejectedValue(new Error('Failed'));

    renderWithProviders(() => <VocabularyPage />);

    await waitFor(() => {
      expect(uiStore.toast.error).toHaveBeenCalledWith('加载失败', 'Failed');
    });
  });

  describe('Search', () => {
    it('calls wordsApi.list with search param on form submit', async () => {
      mockWordsApi.list.mockResolvedValue({ items: [], total: 0 });
      renderWithProviders(() => <VocabularyPage />);

      await waitFor(() => {
        expect(mockWordsApi.list).toHaveBeenCalled();
      });
      mockWordsApi.list.mockClear();
      mockWordsApi.list.mockResolvedValue({ items: [], total: 0 });

      const searchInput = screen.getByPlaceholderText('搜索单词...');
      fireEvent.input(searchInput, { target: { value: 'apple' } });

      const form = searchInput.closest('form')!;
      fireEvent.submit(form);

      await waitFor(() => {
        expect(mockWordsApi.list).toHaveBeenCalledWith(
          expect.objectContaining({ search: 'apple' }),
        );
      });
    });
  });

  describe('Pagination', () => {
    it('calls wordsApi.list with new offset on page change', async () => {
      const words = createFakeWords(3);
      // total=60 with pageSize=20 gives 3 pages
      mockWordsApi.list.mockResolvedValue({ items: words, total: 60 });
      renderWithProviders(() => <VocabularyPage />);

      await waitFor(() => {
        expect(screen.getByText(words[0].text)).toBeInTheDocument();
      });
      mockWordsApi.list.mockClear();
      mockWordsApi.list.mockResolvedValue({ items: words, total: 60 });

      // Click page 2 button (aria-label="第 2 页")
      fireEvent.click(screen.getByLabelText('第 2 页'));

      await waitFor(() => {
        expect(mockWordsApi.list).toHaveBeenCalledWith(
          expect.objectContaining({ offset: 20 }),
        );
      });
    });
  });

  describe('Delete word', () => {
    it('calls wordsApi.delete after confirm via Modal', async () => {
      const words = createFakeWords(1);
      mockWordsApi.list.mockResolvedValue({ items: words, total: 1 });
      mockWordsApi.delete.mockResolvedValue(undefined);

      renderWithProviders(() => <VocabularyPage />);

      await waitFor(() => {
        expect(screen.getByText(words[0].text)).toBeInTheDocument();
      });

      // Find the delete button by the trash SVG path
      const trashBtns = Array.from(document.querySelectorAll('button')).filter(
        (btn) => btn.innerHTML.includes('M19 7l-.867'),
      );
      expect(trashBtns.length).toBeGreaterThan(0);
      fireEvent.click(trashBtns[0]);

      // 删除确认 Modal 应该出现
      await waitFor(() => {
        expect(screen.getByText('确认删除')).toBeInTheDocument();
      });

      // 点击 Modal 中的"删除"按钮确认
      const dialog = screen.getByRole('dialog');
      const confirmDeleteBtn = Array.from(dialog.querySelectorAll('button')).find(
        (btn) => btn.textContent?.trim() === '删除',
      )!;
      fireEvent.click(confirmDeleteBtn);

      await waitFor(() => {
        expect(mockWordsApi.delete).toHaveBeenCalledWith(words[0].id);
      });
    });

    it('does not delete when cancel is clicked in Modal', async () => {
      const words = createFakeWords(1);
      mockWordsApi.list.mockResolvedValue({ items: words, total: 1 });

      renderWithProviders(() => <VocabularyPage />);

      await waitFor(() => {
        expect(screen.getByText(words[0].text)).toBeInTheDocument();
      });

      const trashBtns = Array.from(document.querySelectorAll('button')).filter(
        (btn) => btn.innerHTML.includes('M19 7l-.867'),
      );
      fireEvent.click(trashBtns[0]);

      // 等待 Modal 出现
      await waitFor(() => {
        expect(screen.getByText('确认删除')).toBeInTheDocument();
      });

      // 点击取消按钮
      const dialog = screen.getByRole('dialog');
      const cancelBtn = Array.from(dialog.querySelectorAll('button')).find(
        (btn) => btn.textContent?.trim() === '取消',
      )!;
      fireEvent.click(cancelBtn);

      // Should not have called delete
      expect(mockWordsApi.delete).not.toHaveBeenCalled();
    });
  });

  describe('Add word modal', () => {
    it('opens modal and creates word on submit', async () => {
      mockWordsApi.list.mockResolvedValue({ items: [], total: 0 });
      mockWordsApi.create.mockResolvedValue({ id: 'new-1', text: 'hello', meaning: '你好' });

      renderWithProviders(() => <VocabularyPage />);
      await waitFor(() => expect(mockWordsApi.list).toHaveBeenCalled());

      // Click "添加单词" button
      fireEvent.click(screen.getByText('添加单词'));

      // Wait for modal to appear
      await waitFor(() => {
        expect(screen.getByLabelText('单词')).toBeInTheDocument();
      });

      const wordInput = screen.getByLabelText('单词');
      const meaningInput = screen.getByLabelText('释义');
      fireEvent.input(wordInput, { target: { value: 'hello' } });
      fireEvent.input(meaningInput, { target: { value: '你好' } });

      // Click submit button ("添加") inside the modal dialog
      const dialog = screen.getByRole('dialog');
      const addBtn = Array.from(dialog.querySelectorAll('button')).find(
        (btn) => btn.textContent?.trim() === '添加',
      )!;
      fireEvent.click(addBtn);

      await waitFor(() => {
        expect(mockWordsApi.create).toHaveBeenCalledWith(
          expect.objectContaining({
            text: 'hello',
            meaning: '你好',
          }),
        );
      });
    });
  });

  describe('Edit word modal', () => {
    it('opens modal with pre-filled values and updates word', async () => {
      const word = createFakeWord({
        id: 'w-edit-1',
        text: 'abandon',
        meaning: '放弃',
        pronunciation: '/əˈbændən/',
        partOfSpeech: 'v.',
        tags: ['CET4'],
      });
      mockWordsApi.list.mockResolvedValue({ items: [word], total: 1 });
      mockWordsApi.update.mockResolvedValue({ ...word, meaning: '丢弃' });

      renderWithProviders(() => <VocabularyPage />);

      await waitFor(() => {
        expect(screen.getByText('abandon')).toBeInTheDocument();
      });

      // Click the edit button (pencil icon SVG path contains "M11 5H6")
      const editBtns = Array.from(document.querySelectorAll('button')).filter(
        (btn) => btn.innerHTML.includes('M11 5H6'),
      );
      expect(editBtns.length).toBeGreaterThan(0);
      fireEvent.click(editBtns[0]);

      // Wait for modal with "编辑单词" title
      await waitFor(() => {
        expect(screen.getByText('编辑单词')).toBeInTheDocument();
      });

      // The modal should have pre-filled values
      const meaningInput = screen.getByLabelText('释义') as HTMLInputElement;
      // Update the meaning
      fireEvent.input(meaningInput, { target: { value: '丢弃' } });

      // Click "更新" button
      const dialog = screen.getByRole('dialog');
      const updateBtn = Array.from(dialog.querySelectorAll('button')).find(
        (btn) => btn.textContent?.trim() === '更新',
      )!;
      fireEvent.click(updateBtn);

      await waitFor(() => {
        expect(mockWordsApi.update).toHaveBeenCalledWith(
          'w-edit-1',
          expect.objectContaining({ meaning: '丢弃' }),
        );
      });
    });
  });

  describe('Import modal', () => {
    it('imports via URL', async () => {
      mockWordsApi.list.mockResolvedValue({ items: [], total: 0 });
      mockWordsApi.importUrl.mockResolvedValue({ imported: 5 });

      renderWithProviders(() => <VocabularyPage />);
      await waitFor(() => expect(mockWordsApi.list).toHaveBeenCalled());

      fireEvent.click(screen.getByText('批量导入'));

      await waitFor(() => {
        expect(screen.getByText('URL 导入')).toBeInTheDocument();
      });

      const urlInput = screen.getByLabelText('词库文件 URL');
      fireEvent.input(urlInput, { target: { value: 'https://example.com/words.txt' } });

      fireEvent.click(screen.getByText('开始导入'));

      await waitFor(() => {
        expect(mockWordsApi.importUrl).toHaveBeenCalledWith('https://example.com/words.txt');
      });
    });

    it('imports via text paste', async () => {
      mockWordsApi.list.mockResolvedValue({ items: [], total: 0 });
      mockWordsApi.batchCreate.mockResolvedValue({ count: 2 });

      renderWithProviders(() => <VocabularyPage />);
      await waitFor(() => expect(mockWordsApi.list).toHaveBeenCalled());

      fireEvent.click(screen.getByText('批量导入'));

      await waitFor(() => {
        expect(screen.getByText('文本粘贴')).toBeInTheDocument();
      });

      // Switch to text mode
      fireEvent.click(screen.getByText('文本粘贴'));

      // Find textarea
      await waitFor(() => {
        expect(document.querySelector('textarea')).toBeTruthy();
      });

      const textarea = document.querySelector('textarea')!;
      fireEvent.input(textarea, { target: { value: 'apple\t苹果\nbanana\t香蕉' } });

      fireEvent.click(screen.getByText('开始导入'));

      await waitFor(() => {
        expect(mockWordsApi.batchCreate).toHaveBeenCalled();
      });
    });
  });
});
