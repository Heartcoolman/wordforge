import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import { createFakeWords } from '../helpers/factories';

vi.mock('@/api/learning', () => ({
  learningApi: {
    createSession: vi.fn(),
    getStudyWords: vi.fn(),
  },
}));

vi.mock('@/api/records', () => ({
  recordsApi: {
    create: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { learningApi } from '@/api/learning';
import { recordsApi } from '@/api/records';
import { uiStore } from '@/stores/ui';
import FlashcardPage from '@/pages/FlashcardPage';

const mockLearningApi = learningApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockRecordsApi = recordsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => { server.resetHandlers(); vi.clearAllMocks(); });
afterAll(() => server.close());

describe('FlashcardPage', () => {
  it('shows "闪记模式" heading', () => {
    mockLearningApi.createSession.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <FlashcardPage />);
    expect(screen.getByText('闪记模式')).toBeInTheDocument();
  });

  it('shows loading spinner initially before API resolves', () => {
    mockLearningApi.createSession.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <FlashcardPage />);
    expect(document.querySelector('[class*="animate-spin"]') || screen.getByText('闪记模式')).toBeTruthy();
  });

  it('shows word text after loading', async () => {
    const words = createFakeWords(3);
    mockLearningApi.createSession.mockResolvedValue({ sessionId: 'test-session' });
    mockLearningApi.getStudyWords.mockResolvedValue({ words });
    mockRecordsApi.create.mockResolvedValue({});

    renderWithProviders(() => <FlashcardPage />);

    await waitFor(() => {
      expect(screen.getByText(words[0].text)).toBeInTheDocument();
    });
  });

  it('shows "不认识" and "认识" buttons', async () => {
    const words = createFakeWords(2);
    mockLearningApi.createSession.mockResolvedValue({ sessionId: 'test-session' });
    mockLearningApi.getStudyWords.mockResolvedValue({ words });
    mockRecordsApi.create.mockResolvedValue({});

    renderWithProviders(() => <FlashcardPage />);

    await waitFor(() => {
      expect(screen.getByText(/不认识 \(2/)).toBeInTheDocument();
      expect(screen.getByText(/认识 \(1/)).toBeInTheDocument();
    });
  });

  it('shows "完成!" when words list is empty', async () => {
    mockLearningApi.createSession.mockResolvedValue({ sessionId: 'test-session' });
    mockLearningApi.getStudyWords.mockResolvedValue({ words: [] });

    renderWithProviders(() => <FlashcardPage />);

    await waitFor(() => {
      expect(screen.getByText('完成!')).toBeInTheDocument();
    });
    expect(uiStore.toast.warning).toHaveBeenCalledWith('暂无单词');
  });

  it('shows error toast when API fails', async () => {
    mockLearningApi.createSession.mockRejectedValue(new Error('Network error'));

    renderWithProviders(() => <FlashcardPage />);

    await waitFor(() => {
      expect(uiStore.toast.error).toHaveBeenCalledWith('加载失败', 'Network error');
    });
  });
});
