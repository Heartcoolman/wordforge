import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import type { Wordbook } from '@/types/wordbook';

vi.mock('@/api/wordbooks', () => ({
  wordbooksApi: {
    getSystem: vi.fn(),
    getUser: vi.fn(),
    create: vi.fn(),
  },
}));

vi.mock('@/api/studyConfig', () => ({
  studyConfigApi: {
    get: vi.fn(),
    update: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { wordbooksApi } from '@/api/wordbooks';
import { studyConfigApi } from '@/api/studyConfig';
import { uiStore } from '@/stores/ui';
import WordbookPage from '@/pages/WordbookPage';

const mockWordbooksApi = wordbooksApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockStudyConfigApi = studyConfigApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

function createFakeWordbook(overrides?: Partial<Wordbook>): Wordbook {
  return {
    id: `wb-${Math.random().toString(36).slice(2)}`,
    name: 'Test Book',
    description: 'A test wordbook',
    bookType: 'System' as const,
    wordCount: 100,
    userId: 'user-1',
    createdAt: new Date().toISOString(),
    ...overrides,
  };
}

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => { server.resetHandlers(); vi.clearAllMocks(); });
afterAll(() => server.close());

describe('WordbookPage', () => {
  it('shows "词书管理" heading', () => {
    mockWordbooksApi.getSystem.mockReturnValue(new Promise(() => {}));
    mockWordbooksApi.getUser.mockReturnValue(new Promise(() => {}));
    mockStudyConfigApi.get.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <WordbookPage />);
    expect(screen.getByText('词书管理')).toBeInTheDocument();
  });

  it('shows "创建词书" button', () => {
    mockWordbooksApi.getSystem.mockReturnValue(new Promise(() => {}));
    mockWordbooksApi.getUser.mockReturnValue(new Promise(() => {}));
    mockStudyConfigApi.get.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <WordbookPage />);
    expect(screen.getByText('创建词书')).toBeInTheDocument();
  });

  it('shows loading spinner initially', () => {
    mockWordbooksApi.getSystem.mockReturnValue(new Promise(() => {}));
    mockWordbooksApi.getUser.mockReturnValue(new Promise(() => {}));
    mockStudyConfigApi.get.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <WordbookPage />);
    // While loading, heading and button are visible but content area shows spinner
    expect(screen.getByText('词书管理')).toBeInTheDocument();
  });

  it('shows system wordbooks section', async () => {
    const sysBooks = [
      createFakeWordbook({ name: 'CET4 词汇', bookType: 'System', wordCount: 2000 }),
      createFakeWordbook({ name: 'CET6 词汇', bookType: 'System', wordCount: 3000 }),
    ];
    mockWordbooksApi.getSystem.mockResolvedValue(sysBooks);
    mockWordbooksApi.getUser.mockResolvedValue([]);
    mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: [] });

    renderWithProviders(() => <WordbookPage />);

    await waitFor(() => {
      expect(screen.getByText('系统词书')).toBeInTheDocument();
      expect(screen.getByText('CET4 词汇')).toBeInTheDocument();
      expect(screen.getByText('CET6 词汇')).toBeInTheDocument();
    });
  });

  it('shows empty custom wordbook state', async () => {
    mockWordbooksApi.getSystem.mockResolvedValue([]);
    mockWordbooksApi.getUser.mockResolvedValue([]);
    mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: [] });

    renderWithProviders(() => <WordbookPage />);

    await waitFor(() => {
      expect(screen.getByText('还没有自定义词书')).toBeInTheDocument();
    });
  });

  it('shows error toast when API fails', async () => {
    mockWordbooksApi.getSystem.mockRejectedValue(new Error('Network error'));
    mockWordbooksApi.getUser.mockResolvedValue([]);
    mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: [] });

    renderWithProviders(() => <WordbookPage />);

    await waitFor(() => {
      expect(uiStore.toast.error).toHaveBeenCalledWith('加载失败', 'Network error');
    });
  });

  describe('Toggle select', () => {
    it('calls studyConfigApi.update when clicking a wordbook card', async () => {
      const book = createFakeWordbook({ id: 'wb-1', name: 'CET4', bookType: 'System' });
      mockWordbooksApi.getSystem.mockResolvedValue([book]);
      mockWordbooksApi.getUser.mockResolvedValue([]);
      mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: [] });
      mockStudyConfigApi.update.mockResolvedValue(undefined);

      renderWithProviders(() => <WordbookPage />);

      await waitFor(() => {
        expect(screen.getByText('CET4')).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText('CET4'));

      await waitFor(() => {
        expect(mockStudyConfigApi.update).toHaveBeenCalledWith({
          selectedWordbookIds: ['wb-1'],
        });
      });
    });

    it('deselects when clicking an already-selected book', async () => {
      const book = createFakeWordbook({ id: 'wb-1', name: 'CET4', bookType: 'System' });
      mockWordbooksApi.getSystem.mockResolvedValue([book]);
      mockWordbooksApi.getUser.mockResolvedValue([]);
      mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: ['wb-1'] });
      mockStudyConfigApi.update.mockResolvedValue(undefined);

      renderWithProviders(() => <WordbookPage />);

      await waitFor(() => {
        expect(screen.getByText('CET4')).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText('CET4'));

      await waitFor(() => {
        expect(mockStudyConfigApi.update).toHaveBeenCalledWith({
          selectedWordbookIds: [],
        });
      });
    });
  });

  describe('Selected count', () => {
    it('shows selected count when books are selected', async () => {
      const book = createFakeWordbook({ id: 'wb-1', name: 'CET4', bookType: 'System' });
      mockWordbooksApi.getSystem.mockResolvedValue([book]);
      mockWordbooksApi.getUser.mockResolvedValue([]);
      mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: ['wb-1'] });

      renderWithProviders(() => <WordbookPage />);

      await waitFor(() => {
        expect(screen.getByText(/已选择 1 本词书/)).toBeInTheDocument();
      });
    });
  });

  describe('Create wordbook modal', () => {
    it('creates a wordbook on form submit', async () => {
      mockWordbooksApi.getSystem.mockResolvedValue([]);
      mockWordbooksApi.getUser.mockResolvedValue([]);
      mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: [] });
      mockWordbooksApi.create.mockResolvedValue({ id: 'new-1', name: 'GRE词汇' });

      renderWithProviders(() => <WordbookPage />);

      await waitFor(() => {
        expect(screen.getByText('还没有自定义词书')).toBeInTheDocument();
      });

      // Click "创建词书" button in the header (not the one in the empty state)
      const createBtns = screen.getAllByText('创建词书');
      fireEvent.click(createBtns[0]);

      await waitFor(() => {
        expect(screen.getByLabelText('名称')).toBeInTheDocument();
      });

      fireEvent.input(screen.getByLabelText('名称'), { target: { value: 'GRE词汇' } });

      // Click the "创建" button inside the modal dialog
      const dialog = screen.getByRole('dialog');
      const submitBtn = Array.from(dialog.querySelectorAll('button')).find(
        (btn) => btn.textContent?.trim() === '创建',
      )!;
      fireEvent.click(submitBtn);

      await waitFor(() => {
        expect(mockWordbooksApi.create).toHaveBeenCalledWith(
          expect.objectContaining({ name: 'GRE词汇' }),
        );
      });
    });
  });

  describe('Optimistic rollback', () => {
    it('reverts selection and shows error on update failure', async () => {
      const book = createFakeWordbook({ id: 'wb-1', name: 'CET4', bookType: 'System' });
      mockWordbooksApi.getSystem.mockResolvedValue([book]);
      mockWordbooksApi.getUser.mockResolvedValue([]);
      mockStudyConfigApi.get.mockResolvedValue({ selectedWordbookIds: [] });
      mockStudyConfigApi.update.mockRejectedValue(new Error('Network fail'));

      renderWithProviders(() => <WordbookPage />);

      await waitFor(() => {
        expect(screen.getByText('CET4')).toBeInTheDocument();
      });

      fireEvent.click(screen.getByText('CET4'));

      await waitFor(() => {
        expect(uiStore.toast.error).toHaveBeenCalledWith('更新失败');
      });
    });
  });
});
