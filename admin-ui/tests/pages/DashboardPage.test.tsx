import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getStats: vi.fn(),
    getEngagement: vi.fn(),
    getStudyOverview: vi.fn(),
    getDailyActiveUsers: vi.fn(),
    getDailyRecords: vi.fn(),
    getHealth: vi.fn(),
    checkUpdate: vi.fn(),
    // m023:Dashboard 新接的端点(默认空 / null,具体用例可 override)
    amasMetricsTimeseries: vi.fn(() => Promise.resolve([])),
    analyticsHourly: vi.fn(() => Promise.resolve(null)),
    monitoringWorkers: vi.fn(() => Promise.resolve({ workers: [] })),
    amasListSuggestions: vi.fn(() => Promise.resolve([])),
    listFeedback: vi.fn(() => Promise.resolve({ data: [], total: 0, page: 1, perPage: 1, totalPages: 0 })),
  },
}));
vi.mock('@/api/amas', () => ({
  amasApi: {
    getMonitoring: vi.fn(() => Promise.resolve([])),
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

describe('DashboardPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: DashboardPage } = await import('@/pages/DashboardPage');
    return renderWithProviders(() => <DashboardPage />);
  }

  function primeSuccessMocks() {
    mockAdminApi.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000, trend: {} });
    mockAdminApi.getEngagement.mockResolvedValue({ totalUsers: 100, activeToday: 12, retentionRate: 0.12, trend: {} });
    mockAdminApi.getStudyOverview.mockResolvedValue({
      generatedAt: '2026-04-15T00:00:00Z',
      days: 7,
      category: 'all',
      summary: {
        totalDurationSecs: 7200, sessionCount: 30, recordCount: 500, correctCount: 410,
        accuracy: 0.82, newWords: 45, reviewWords: 120, masteredWords: 18,
      },
      daily: [],
    });
    mockAdminApi.getDailyActiveUsers.mockResolvedValue([{ date: '2026-04-15', count: 12, registered: 1 }]);
    mockAdminApi.getDailyRecords.mockResolvedValue([{ date: '2026-04-15', correct: 50, total: 60, durationSecs: 600, newWords: 8 }]);
    mockAdminApi.getHealth.mockResolvedValue({ status: 'healthy', dbSizeBytes: 1048576, uptimeSecs: 7200, version: '1.0.0' });
    mockAdminApi.checkUpdate.mockResolvedValue({ currentVersion: '1.0.0', latestVersion: '1.0.0', hasUpdate: false, releaseUrl: null, releaseNotes: null });
  }

  it('renders global overview header', async () => {
    primeSuccessMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('全局概览')).toBeInTheDocument();
    });
  });

  it('renders KPI cards after loading', async () => {
    primeSuccessMocks();
    await renderPage();
    // PR redesign: HeroCard meta + StatCard 都展示这些 label，所以 getAllByText
    await waitFor(() => {
      expect(screen.getAllByText('注册用户').length).toBeGreaterThan(0);
    });
    expect(screen.getAllByText('今日活跃').length).toBeGreaterThan(0);
  });

  it('renders panel titles', async () => {
    primeSuccessMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('用户活跃趋势')).toBeInTheDocument();
    });
    expect(screen.getByText('学习产出')).toBeInTheDocument();
  });

  it('shows system status section after loading', async () => {
    primeSuccessMocks();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('系统状态')).toBeInTheDocument();
    });
    // PR redesign: '运行正常' 同时出现在 HeroCard eyebrow 和系统状态卡里
    expect(screen.getAllByText('运行正常').length).toBeGreaterThan(0);
  });
});
