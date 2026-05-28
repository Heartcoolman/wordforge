import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/components/ui/EChart', () => ({
  EChart: () => <div data-testid="chart" />,
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
const mockLearning = { totalWords: 5000, totalRecords: 20000, totalCorrect: 17000, overallAccuracy: 0.85, trend: {} };
const mockOverview = {
  summary: { totalDurationSecs: 3600, sessionCount: 10, recordCount: 200, correctCount: 170, accuracy: 0.85, newWords: 30, reviewWords: 80, masteredWords: 5 },
  daily: [],
};
const mockRecordTypes = {
  daily: [{ date: '2026-04-15', learning: 10, review: 20, all: 5 }],
  totals: { learning: 10, review: 20, all: 5 },
};
const mockRetention = {
  averageRetention: 0.8,
  points: [
    { daysSinceLearn: 1, retention: 0.95, sampleSize: 100 },
    { daysSinceLearn: 7, retention: 0.85, sampleSize: 80 },
  ],
};
const mockStates = {
  generatedAt: '2026-04-15T00:00:00Z',
  category: 'all',
  states: { newCount: 100, learning: 200, reviewing: 150, mastered: 50, forgotten: 30 },
  totals: { trackedWords: 530, bookmarkedWords: 20, dueReviewWords: 15, overdueReviewWords: 3, averageMasteryLevel: 0.6 },
};

async function renderPage() {
  const { default: Page } = await import('@/pages/AnalyticsPage');
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

describe('AnalyticsPage — empty data, loading state, days picker', () => {
  beforeEach(() => vi.clearAllMocks());

  it('still renders headings when recordTypes data is empty', async () => {
    primeAll();
    mockApi.getRecordTypes.mockResolvedValue({ daily: [], totals: { learning: 0, review: 0, all: 0 } });
    await renderPage();
    await waitFor(() => expect(screen.getByText('学习构成（学习 vs 复习）')).toBeInTheDocument());
  });

  it('renders retention loading skeleton when promise pending', async () => {
    primeAll();
    mockApi.getRetentionCurve.mockReturnValue(new Promise(() => {}));
    await renderPage();
    await waitFor(() => expect(screen.getByText('记忆遗忘曲线')).toBeInTheDocument());
  });

  it('setDays via WindowPicker triggers refetch with new window', async () => {
    primeAll();
    await renderPage();
    await waitFor(() => expect(screen.getByText('30天')).toBeInTheDocument());
    fireEvent.click(screen.getByText('30天'));
    await waitFor(() => {
      expect(mockApi.getStudyOverview).toHaveBeenCalledWith(30);
    });
  });
});
