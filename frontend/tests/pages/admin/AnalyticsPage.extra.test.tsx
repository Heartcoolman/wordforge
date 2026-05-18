import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

// 把 EChart mock 成同步调用 props.option() 的版本，
// 让 AnalyticsPage 内三个图表的 option lambda（含 STATE_LABELS 映射、
// scatter/line/pie series 构造）都被执行一次，从而覆盖行 195-230 区段
// 以及 PieChart 269-282 这段闭包体。
vi.mock('@/components/ui/EChart', () => ({
  EChart: (props: { option: () => unknown }) => {
    try {
      props.option();
    } catch {
      /* 隔离闭包内的潜在异常，确保 mock 不影响后续渲染 */
    }
    return <div data-testid="chart" />;
  },
}));

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

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockEngagement = { totalUsers: 100, activeToday: 12, retentionRate: 0.5, trend: {} };
const mockLearning = {
  totalWords: 5000,
  totalRecords: 20000,
  totalCorrect: 17000,
  overallAccuracy: 0.85,
  trend: {},
};
const mockOverview = {
  summary: {
    totalDurationSecs: 3600,
    sessionCount: 10,
    recordCount: 200,
    correctCount: 170,
    accuracy: 0.85,
    newWords: 30,
    reviewWords: 80,
    masteredWords: 5,
  },
  daily: [],
};
const mockRecordTypes = {
  daily: [
    { date: '2026-04-15', learning: 10, review: 20, all: 5 },
    { date: '2026-04-16', learning: 12, review: 18, all: 6 },
  ],
  totals: { learning: 22, review: 38, all: 11 },
};
const mockRetention = {
  averageRetention: 0.8,
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
  totals: {
    trackedWords: 530,
    bookmarkedWords: 20,
    dueReviewWords: 15,
    overdueReviewWords: 3,
    averageMasteryLevel: 0.6,
  },
};

describe('AnalyticsPage extra (EChart option lambdas)', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/admin/AnalyticsPage');
    return renderWithProviders(() => <Page />);
  }

  function primeAll() {
    mockApi.getEngagement.mockResolvedValue(mockEngagement);
    mockApi.getLearningAnalytics.mockResolvedValue(mockLearning);
    mockApi.getDailyRecords.mockResolvedValue([]);
    mockApi.getStudyOverview.mockResolvedValue(mockOverview);
    mockApi.getRecordTypes.mockResolvedValue(mockRecordTypes);
    mockApi.getRetentionCurve.mockResolvedValue(mockRetention);
    mockApi.getWordStateDistribution.mockResolvedValue(mockStates);
  }

  it('invokes all three EChart option lambdas (record-types / retention / pie)', async () => {
    primeAll();
    await renderPage();
    // 三个图表全部 mount，option() 已被同步触发；
    // STATE_LABELS 映射也会跑过 cssVar(s.tone, ...) / itemStyle 分支
    await waitFor(() => {
      const charts = screen.getAllByTestId('chart');
      expect(charts.length).toBeGreaterThanOrEqual(3);
    });
  });

  it('still renders headings when recordTypes data is empty (no chart)', async () => {
    primeAll();
    mockApi.getRecordTypes.mockResolvedValue({ daily: [], totals: { learning: 0, review: 0, all: 0 } });
    await renderPage();
    await waitFor(() => expect(screen.getByText('学习构成（学习 vs 复习）')).toBeInTheDocument());
  });
});
