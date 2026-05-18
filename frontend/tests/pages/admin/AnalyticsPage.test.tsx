import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
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

const mockEngagement = {
  totalUsers: 500,
  activeToday: 42,
  retentionRate: 0.75,
  trend: { activeToday: 5 },
};
const mockLearning = {
  totalWords: 8000,
  totalRecords: 50000,
  totalCorrect: 40000,
  overallAccuracy: 0.8,
  trend: { totalRecords: 12, overallAccuracy: -1 },
};
const mockOverview = {
  summary: {
    totalDurationSecs: 7200,
    sessionCount: 30,
    recordCount: 500,
    correctCount: 410,
    accuracy: 0.82,
    newWords: 45,
    reviewWords: 120,
    masteredWords: 18,
  },
  daily: [{ date: '2026-04-15', durationSecs: 600, sessionCount: 5, recordCount: 60, correctCount: 50, accuracy: 0.83, newWords: 5, reviewWords: 12, masteredWords: 2 }],
};
const mockRecordTypes = {
  daily: [
    { date: '2026-04-15', learning: 10, review: 20, all: 5 },
    { date: '2026-04-16', learning: 8, review: 18, all: 4 },
  ],
  totals: { learning: 18, review: 38, all: 9 },
};
const mockRetention = {
  averageRetention: 0.85,
  points: [
    { daysSinceLearn: 1, retention: 0.95, sampleSize: 100 },
    { daysSinceLearn: 7, retention: 0.85, sampleSize: 80 },
    { daysSinceLearn: 14, retention: null, sampleSize: 0 },
  ],
};
const mockStates = {
  generatedAt: '2026-04-15T00:00:00Z',
  category: 'all',
  states: { newCount: 100, learning: 200, reviewing: 150, mastered: 50, forgotten: 30 },
  totals: { trackedWords: 530, bookmarkedWords: 20, dueReviewWords: 15, overdueReviewWords: 3, averageMasteryLevel: 0.6 },
};

const mockEmptyRetention = {
  averageRetention: null,
  points: [
    { daysSinceLearn: 1, retention: null, sampleSize: 0 },
    { daysSinceLearn: 7, retention: null, sampleSize: 0 },
  ],
};
const mockEmptyStates = {
  generatedAt: '2026-04-15T00:00:00Z',
  category: 'all',
  states: { newCount: 0, learning: 0, reviewing: 0, mastered: 0, forgotten: 0 },
  totals: { trackedWords: 0, bookmarkedWords: 0, dueReviewWords: 0, overdueReviewWords: 0, averageMasteryLevel: null },
};

describe('AnalyticsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function primeMocks() {
    mockAdminApi.getEngagement.mockResolvedValue(mockEngagement);
    mockAdminApi.getLearningAnalytics.mockResolvedValue(mockLearning);
    mockAdminApi.getDailyRecords.mockResolvedValue([{ date: '2026-04-15', correct: 10, total: 12 }]);
    mockAdminApi.getStudyOverview.mockResolvedValue(mockOverview);
    mockAdminApi.getRecordTypes.mockResolvedValue(mockRecordTypes);
    mockAdminApi.getRetentionCurve.mockResolvedValue(mockRetention);
    mockAdminApi.getWordStateDistribution.mockResolvedValue(mockStates);
  }

  function primeEmpty() {
    mockAdminApi.getEngagement.mockResolvedValue(mockEngagement);
    mockAdminApi.getLearningAnalytics.mockResolvedValue(mockLearning);
    mockAdminApi.getDailyRecords.mockResolvedValue([]);
    mockAdminApi.getStudyOverview.mockResolvedValue(mockOverview);
    mockAdminApi.getRecordTypes.mockResolvedValue({ daily: [], totals: { learning: 0, review: 0, all: 0 } });
    mockAdminApi.getRetentionCurve.mockResolvedValue(mockEmptyRetention);
    mockAdminApi.getWordStateDistribution.mockResolvedValue(mockEmptyStates);
  }

  async function renderPage() {
    const { default: AnalyticsPage } = await import('@/pages/admin/AnalyticsPage');
    return renderWithProviders(() => <AnalyticsPage />);
  }

  it('renders page heading after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('深度分析')).toBeInTheDocument());
  });

  it('shows KPI cards after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('总用户')).toBeInTheDocument());
    expect(screen.getByText('今日活跃')).toBeInTheDocument();
    expect(screen.getByText('累计答题')).toBeInTheDocument();
    expect(screen.getByText('累计正确率')).toBeInTheDocument();
  });

  it('renders 学习构成 chart when data has entries', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('学习构成（学习 vs 复习）')).toBeInTheDocument());
  });

  it('renders 记忆遗忘曲线 with averageRetention chip', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('记忆遗忘曲线')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText(/平均留存/)).toBeInTheDocument());
  });

  it('renders 单词状态分布 with totals chip and pie list', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('单词状态分布')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText(/跟踪/)).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText(/已掌握/)).toBeInTheDocument());
  });

  it('shows empty state for retention when all samples are zero', async () => {
    primeEmpty();
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无样本')).toBeInTheDocument());
  });

  it('shows empty state for states when total is zero', async () => {
    primeEmpty();
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无数据')).toBeInTheDocument());
  });

  it('shows error empty when recordTypes fetch rejects', async () => {
    mockAdminApi.getEngagement.mockResolvedValue(mockEngagement);
    mockAdminApi.getLearningAnalytics.mockResolvedValue(mockLearning);
    mockAdminApi.getStudyOverview.mockResolvedValue(mockOverview);
    mockAdminApi.getRecordTypes.mockRejectedValue(new Error('boom'));
    mockAdminApi.getRetentionCurve.mockResolvedValue(mockRetention);
    mockAdminApi.getWordStateDistribution.mockResolvedValue(mockStates);
    await renderPage();
    await waitFor(() => {
      const failed = screen.getAllByText('加载失败');
      expect(failed.length).toBeGreaterThan(0);
    });
  });

  it('updates window via search params when picker changes', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('深度分析')).toBeInTheDocument());
    const btn14 = screen.getAllByRole('button').find((b) => b.textContent === '14 天');
    if (btn14) {
      fireEvent.click(btn14);
      await waitFor(() => expect(mockAdminApi.getStudyOverview).toHaveBeenCalledWith(14));
    }
  });
});
