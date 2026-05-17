import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getEngagement: vi.fn(),
    getLearningAnalytics: vi.fn(),
    getDailyRecords: vi.fn(),
    getStudyOverview: vi.fn(),
    getRecordTypes: vi.fn(),
    getRetentionCurve: vi.fn(),
    getWordStateDistribution: vi.fn(),
  },
}));
vi.mock('@/components/ui/EChart', () => ({
  EChart: () => <div data-testid="chart" />,
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockEngagement = { totalUsers: 500, activeToday: 42, retentionRate: 0.75 };
const mockLearning = { totalWords: 8000, totalRecords: 50000, totalCorrect: 40000, overallAccuracy: 0.8 };
const mockDailyRecords = [{ date: '2026-04-15', correct: 10, total: 12 }];
const mockOverview = { summary: { totalDurationSecs: 0, newWords: 0 }, daily: [] };
const mockRecordTypes = { daily: [], totals: { learning: 0, review: 0, all: 0 } };
const mockRetention = { points: [], averageRetention: null };
const mockStates = {
  tracked: { NEW: 0, LEARNING: 0, REVIEWING: 0, MASTERED: 0, FORGOTTEN: 0 },
  bookmarked: { NEW: 0, LEARNING: 0, REVIEWING: 0, MASTERED: 0, FORGOTTEN: 0 },
};

describe('AnalyticsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function primeMocks() {
    mockAdminApi.getEngagement.mockResolvedValue(mockEngagement);
    mockAdminApi.getLearningAnalytics.mockResolvedValue(mockLearning);
    mockAdminApi.getDailyRecords.mockResolvedValue(mockDailyRecords);
    mockAdminApi.getStudyOverview.mockResolvedValue(mockOverview);
    mockAdminApi.getRecordTypes.mockResolvedValue(mockRecordTypes);
    mockAdminApi.getRetentionCurve.mockResolvedValue(mockRetention);
    mockAdminApi.getWordStateDistribution.mockResolvedValue(mockStates);
  }

  async function renderPage() {
    const { default: AnalyticsPage } = await import('@/pages/admin/AnalyticsPage');
    return renderWithProviders(() => <AnalyticsPage />);
  }

  it('renders page heading after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('深度分析')).toBeInTheDocument();
    });
  });

  it('shows top-level stat cards after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('总用户')).toBeInTheDocument();
    });
    expect(screen.getByText('今日活跃')).toBeInTheDocument();
    expect(screen.getByText('累计答题')).toBeInTheDocument();
    expect(screen.getByText('累计正确率')).toBeInTheDocument();
  });

  it('shows 学习构成 section after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('学习构成（学习 vs 复习）')).toBeInTheDocument();
    });
  });

  it('shows 记忆遗忘曲线 section after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('记忆遗忘曲线')).toBeInTheDocument();
    });
  });

  it('shows 单词状态分布 section after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('单词状态分布')).toBeInTheDocument();
    });
  });

  it('renders without throwing when all resources are empty', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('深度分析')).toBeInTheDocument();
    });
  });
});
