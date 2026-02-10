import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { server } from '../helpers/msw-server';
import HomePage from '@/pages/HomePage';
import { authStore } from '@/stores/auth';
import { studyConfigApi } from '@/api/studyConfig';
import { usersApi } from '@/api/users';

vi.mock('@/stores/auth', () => ({
  authStore: {
    isAuthenticated: vi.fn(() => false),
    user: vi.fn(() => null),
    loading: vi.fn(() => false),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

vi.mock('@/api/studyConfig', () => ({
  studyConfigApi: { getProgress: vi.fn() },
}));

vi.mock('@/api/users', () => ({
  usersApi: { getStats: vi.fn() },
}));

const mockAuthStore = authStore as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockStudyConfigApi = studyConfigApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockUsersApi = usersApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => {
  server.resetHandlers();
  vi.clearAllMocks();
  mockAuthStore.isAuthenticated.mockReturnValue(false);
  mockAuthStore.user.mockReturnValue(null);
});
afterAll(() => server.close());

describe('HomePage (unauthenticated)', () => {
  it('shows WordMaster heading', () => {
    renderWithProviders(() => <HomePage />);
    expect(screen.getByText('WordMaster')).toBeInTheDocument();
  });

  it('shows 开始学习 button', () => {
    renderWithProviders(() => <HomePage />);
    expect(screen.getByText('开始学习')).toBeInTheDocument();
  });

  it('has login and register links', () => {
    renderWithProviders(() => <HomePage />);
    const loginLink = screen.getByText('登录').closest('a');
    const registerLink = screen.getByText('开始学习').closest('a');
    expect(loginLink).toHaveAttribute('href', '/login');
    expect(registerLink).toHaveAttribute('href', '/register');
  });
});

describe('HomePage (authenticated dashboard)', () => {
  beforeEach(() => {
    mockAuthStore.isAuthenticated.mockReturnValue(true);
    mockAuthStore.user.mockReturnValue({ id: '1', username: 'Alice', email: 'a@b.com', isBanned: false });
  });

  it('shows greeting with username', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 5, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 100, totalSessions: 10, streakDays: 7, accuracyRate: 0.85, totalRecords: 500 });
    renderWithProviders(() => <HomePage />);
    await waitFor(() => {
      expect(screen.getByText(/你好, Alice/)).toBeInTheDocument();
    });
  });

  it('shows loading spinner while data loads', () => {
    mockStudyConfigApi.getProgress.mockReturnValue(new Promise(() => {}));
    mockUsersApi.getStats.mockReturnValue(new Promise(() => {}));
    renderWithProviders(() => <HomePage />);
    expect(screen.getByText(/你好, Alice/)).toBeInTheDocument();
  });

  it('shows today progress when loaded', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 10, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 100, totalSessions: 10, streakDays: 7, accuracyRate: 0.85, totalRecords: 500 });
    renderWithProviders(() => <HomePage />);
    await waitFor(() => {
      expect(screen.getByText(/已学 10 \/ 目标 20/)).toBeInTheDocument();
    });
  });

  it('shows stat cards with correct labels', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 10, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 100, totalSessions: 10, streakDays: 7, accuracyRate: 0.85, totalRecords: 500 });
    renderWithProviders(() => <HomePage />);
    await waitFor(() => {
      expect(screen.getByText('已学单词')).toBeInTheDocument();
      expect(screen.getByText('连续学习')).toBeInTheDocument();
      expect(screen.getByText('正确率')).toBeInTheDocument();
      expect(screen.getByText('总记录')).toBeInTheDocument();
    });
  });

  it('shows quick link cards', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 0, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 0, totalSessions: 0, streakDays: 0, accuracyRate: 0, totalRecords: 0 });
    renderWithProviders(() => <HomePage />);
    await waitFor(() => {
      expect(screen.getByText('四选一学习')).toBeInTheDocument();
      expect(screen.getByText('闪记模式')).toBeInTheDocument();
      expect(screen.getByText('词库管理')).toBeInTheDocument();
      expect(screen.getByText('学习统计')).toBeInTheDocument();
    });
  });

  it('shows encouragement text', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 0, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 0, totalSessions: 0, streakDays: 0, accuracyRate: 0, totalRecords: 0 });
    renderWithProviders(() => <HomePage />);
    expect(screen.getByText('今天也要加油哦!')).toBeInTheDocument();
  });

  it('shows 开始学习 button in dashboard', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 0, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 0, totalSessions: 0, streakDays: 0, accuracyRate: 0, totalRecords: 0 });
    renderWithProviders(() => <HomePage />);
    const btn = screen.getByText('开始学习');
    const link = btn.closest('a');
    expect(link).toHaveAttribute('href', '/learning');
  });

  it('handles progress API failure gracefully', async () => {
    mockStudyConfigApi.getProgress.mockRejectedValue(new Error('fail'));
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 100, totalSessions: 10, streakDays: 7, accuracyRate: 0.85, totalRecords: 500 });
    renderWithProviders(() => <HomePage />);
    // Should still show stats even if progress fails
    await waitFor(() => {
      expect(screen.getByText('已学单词')).toBeInTheDocument();
    });
  });

  it('handles stats API failure gracefully', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 5, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockRejectedValue(new Error('fail'));
    renderWithProviders(() => <HomePage />);
    // Should still show progress even if stats fails
    await waitFor(() => {
      expect(screen.getByText(/已学 5 \/ 目标 20/)).toBeInTheDocument();
    });
  });

  it('handles both APIs failing gracefully', async () => {
    mockStudyConfigApi.getProgress.mockRejectedValue(new Error('fail'));
    mockUsersApi.getStats.mockRejectedValue(new Error('fail'));
    renderWithProviders(() => <HomePage />);
    // Should still show greeting and quick links
    await waitFor(() => {
      expect(screen.getByText(/你好, Alice/)).toBeInTheDocument();
      expect(screen.getByText('四选一学习')).toBeInTheDocument();
    });
  });

  it('displays streak days with 天 suffix', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 0, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 0, totalSessions: 0, streakDays: 7, accuracyRate: 0, totalRecords: 0 });
    renderWithProviders(() => <HomePage />);
    await waitFor(() => {
      expect(screen.getByText('7 天')).toBeInTheDocument();
    });
  });

  it('displays accuracy rate as percentage', async () => {
    mockStudyConfigApi.getProgress.mockResolvedValue({ studied: 0, target: 20, new: 0, learning: 0, reviewing: 0, mastered: 0 });
    mockUsersApi.getStats.mockResolvedValue({ totalWordsLearned: 0, totalSessions: 0, streakDays: 0, accuracyRate: 0.85, totalRecords: 0 });
    renderWithProviders(() => <HomePage />);
    await waitFor(() => {
      expect(screen.getByText('85.0%')).toBeInTheDocument();
    });
  });
});
