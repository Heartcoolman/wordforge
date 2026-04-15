import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getEngagement: vi.fn(),
    getLearningAnalytics: vi.fn(),
    getDailyRecords: vi.fn(),
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

describe('AnalyticsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function primeMocks() {
    mockAdminApi.getEngagement.mockResolvedValue(mockEngagement);
    mockAdminApi.getLearningAnalytics.mockResolvedValue(mockLearning);
    mockAdminApi.getDailyRecords.mockResolvedValue(mockDailyRecords);
  }

  async function renderPage() {
    const { default: AnalyticsPage } = await import('@/pages/admin/AnalyticsPage');
    return renderWithProviders(() => <AnalyticsPage />);
  }

  it('renders analytics sections after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('用户活跃度')).toBeInTheDocument();
    });
  });

  it('shows loading spinner initially', async () => {
    mockAdminApi.getEngagement.mockReturnValue(new Promise(() => {}));
    mockAdminApi.getLearningAnalytics.mockReturnValue(new Promise(() => {}));
    mockAdminApi.getDailyRecords.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows "用户活跃度" section after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('用户活跃度')).toBeInTheDocument();
    });
  });

  it('shows engagement stats values after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('总用户')).toBeInTheDocument();
    });
    expect(screen.getByText('今日活跃')).toBeInTheDocument();
    expect(screen.getByText('日活跃率')).toBeInTheDocument();
  });

  it('shows "学习数据" section after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('学习数据')).toBeInTheDocument();
    });
  });

  it('shows learning stats labels after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('总单词')).toBeInTheDocument();
    });
    expect(screen.getByText('总记录')).toBeInTheDocument();
    expect(screen.getByText('正确数')).toBeInTheDocument();
    expect(screen.getByText('总正确率')).toBeInTheDocument();
  });
});
