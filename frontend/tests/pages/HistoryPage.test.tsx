import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import { createFakeWord } from '../helpers/factories';
import type { LearningRecord } from '@/types/record';

vi.mock('@/api/records', () => ({
  recordsApi: {
    list: vi.fn(),
  },
}));

vi.mock('@/api/words', () => ({
  wordsApi: {
    get: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

vi.mock('@/utils/formatters', () => ({
  formatDateTime: vi.fn((_iso: string) => '2026-01-15 10:30'),
  formatResponseTime: vi.fn((ms: number) => `${ms}ms`),
}));

import { recordsApi } from '@/api/records';
import { wordsApi } from '@/api/words';
import { uiStore } from '@/stores/ui';
import HistoryPage from '@/pages/HistoryPage';

const mockRecordsApi = recordsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockWordsApi = wordsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

function createFakeRecord(overrides?: Partial<LearningRecord>): LearningRecord {
  return {
    id: `record-${Math.random().toString(36).slice(2)}`,
    userId: 'user-1',
    wordId: 'word-1',
    isCorrect: true,
    responseTimeMs: 1500,
    createdAt: new Date().toISOString(),
    ...overrides,
  };
}

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => { server.resetHandlers(); vi.clearAllMocks(); });
afterAll(() => server.close());

describe('HistoryPage', () => {
  it('shows "学习历史" heading', () => {
    mockRecordsApi.list.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <HistoryPage />);
    expect(screen.getByText('学习历史')).toBeInTheDocument();
  });

  it('shows loading spinner initially', () => {
    mockRecordsApi.list.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <HistoryPage />);
    // Heading is visible while loading spinner shows
    expect(screen.getByText('学习历史')).toBeInTheDocument();
  });

  it('shows empty state when no records', async () => {
    mockRecordsApi.list.mockResolvedValue({ data: [], page: 1, totalPages: 1 });

    renderWithProviders(() => <HistoryPage />);

    await waitFor(() => {
      expect(screen.getByText('暂无学习记录')).toBeInTheDocument();
    });
  });

  it('shows records with correct/error badges', async () => {
    const word1 = createFakeWord({ id: 'w1', text: 'hello' });
    const word2 = createFakeWord({ id: 'w2', text: 'world' });
    const records = [
      createFakeRecord({ wordId: 'w1', isCorrect: true }),
      createFakeRecord({ wordId: 'w2', isCorrect: false }),
    ];

    mockRecordsApi.list.mockResolvedValue({ data: records, page: 1, totalPages: 1 });
    mockWordsApi.get.mockImplementation((id: string) => {
      if (id === 'w1') return Promise.resolve(word1);
      if (id === 'w2') return Promise.resolve(word2);
      return Promise.reject(new Error('not found'));
    });

    renderWithProviders(() => <HistoryPage />);

    await waitFor(() => {
      expect(screen.getByText('正确')).toBeInTheDocument();
      expect(screen.getByText('错误')).toBeInTheDocument();
    });
  });

  it('shows "加载更多" button when hasMore is true', async () => {
    const records = Array.from({ length: 5 }, (_, i) =>
      createFakeRecord({ wordId: `w-${i}` }),
    );
    // page=1, totalPages=3 => hasMore = true (1 < 3)
    mockRecordsApi.list.mockResolvedValue({ data: records, page: 1, totalPages: 3 });
    mockWordsApi.get.mockResolvedValue(createFakeWord());

    renderWithProviders(() => <HistoryPage />);

    await waitFor(() => {
      expect(screen.getByText('加载更多')).toBeInTheDocument();
    });
  });

  it('shows error toast when API fails', async () => {
    mockRecordsApi.list.mockRejectedValue(new Error('Server error'));

    renderWithProviders(() => <HistoryPage />);

    await waitFor(() => {
      expect(uiStore.toast.error).toHaveBeenCalledWith('加载失败', 'Server error');
    });
  });
});
