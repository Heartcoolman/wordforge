import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getStats: vi.fn(),
    getEngagement: vi.fn(),
    getStudyOverview: vi.fn(),
    getDailyActiveUsers: vi.fn(),
    getDailyRecords: vi.fn(),
    getHealth: vi.fn(),
    checkUpdate: vi.fn(),
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

describe('AdminDashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: AdminDashboard } = await import('@/pages/admin/AdminDashboard');
    return renderWithProviders(() => <AdminDashboard />);
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
    await waitFor(() => {
      expect(screen.getByText('注册用户')).toBeInTheDocument();
    });
    expect(screen.getByText('今日活跃')).toBeInTheDocument();
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
    expect(screen.getByText('运行正常')).toBeInTheDocument();
  });
});
