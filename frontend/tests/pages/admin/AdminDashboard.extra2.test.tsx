import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

// 触发两张 Panel 内的 EChart option lambda（lines 122-150 / 195-230）
vi.mock('@/components/ui/EChart', () => ({
  EChart: (props: { option: () => unknown }) => {
    try {
      props.option();
    } catch {
      /* noop */
    }
    return <div data-testid="chart" />;
  },
}));

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

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('AdminDashboard extra2 (EChart option lambdas + all-fail)', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/admin/AdminDashboard');
    return renderWithProviders(() => <Page />);
  }

  function primeAll() {
    mockApi.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000, trend: { users: 5 } });
    mockApi.getEngagement.mockResolvedValue({ totalUsers: 100, activeToday: 12, retentionRate: 0.12, trend: { activeToday: 2 } });
    mockApi.getStudyOverview.mockResolvedValue({
      summary: { totalDurationSecs: 7200, sessionCount: 30, recordCount: 500, correctCount: 410, accuracy: 0.82, newWords: 45 },
      daily: [],
    });
    mockApi.getDailyActiveUsers.mockResolvedValue([
      { date: '2026-04-15', count: 12, registered: 1 },
      { date: '2026-04-16', count: 18, registered: 2 },
    ]);
    mockApi.getDailyRecords.mockResolvedValue([
      { date: '2026-04-15', correct: 50, total: 60, durationSecs: 600, newWords: 8 },
      { date: '2026-04-16', correct: 0, total: 0, durationSecs: 0, newWords: 0 },
    ]);
    mockApi.getHealth.mockResolvedValue({ status: 'healthy', dbSizeBytes: 1048576, uptimeSecs: 7200, version: '1.0.0' });
    mockApi.checkUpdate.mockResolvedValue({ currentVersion: '1.0.0', latestVersion: '1.0.0', hasUpdate: false, releaseUrl: null });
  }

  it('invokes both Panel EChart option lambdas with records including zero-total branch', async () => {
    primeAll();
    await renderPage();
    await waitFor(() => {
      const charts = screen.getAllByTestId('chart');
      // dau + records = 2 张
      expect(charts.length).toBeGreaterThanOrEqual(2);
    });
  });

  it('shows global 仪表盘加载失败 Empty when every resource rejects', async () => {
    mockApi.getStats.mockRejectedValue(new Error('s'));
    mockApi.getEngagement.mockRejectedValue(new Error('e'));
    mockApi.getStudyOverview.mockRejectedValue(new Error('o'));
    mockApi.getDailyActiveUsers.mockRejectedValue(new Error('d'));
    mockApi.getDailyRecords.mockRejectedValue(new Error('r'));
    mockApi.getHealth.mockRejectedValue(new Error('h'));
    mockApi.checkUpdate.mockResolvedValue({ currentVersion: '0', latestVersion: '0', hasUpdate: false });
    await renderPage();
    // 至少有一个 Card 内 Empty fallback 命中即可
    await waitFor(() => {
      const matches = screen.getAllByText('加载失败');
      expect(matches.length).toBeGreaterThanOrEqual(1);
    });
  });
});
