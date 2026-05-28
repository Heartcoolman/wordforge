import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
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
  },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockHealth = (status: string) => ({ status, dbSizeBytes: 1048576, uptimeSecs: 7200, version: '1.0.0' });

async function renderPage() {
  const { default: Page } = await import('@/pages/DashboardPage');
  return renderWithProviders(() => <Page />);
}

function primeBase() {
  mockApi.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000, trend: { users: 5 } });
  mockApi.getEngagement.mockResolvedValue({ totalUsers: 100, activeToday: 12, retentionRate: 0.12, trend: { activeToday: 2 } });
  mockApi.getStudyOverview.mockResolvedValue({
    summary: { totalDurationSecs: 7200, sessionCount: 30, recordCount: 500, correctCount: 410, accuracy: 0.82, newWords: 45 },
    daily: [],
  });
  mockApi.getDailyActiveUsers.mockResolvedValue([{ date: '2026-04-15', count: 12, registered: 1 }]);
  mockApi.getDailyRecords.mockResolvedValue([{ date: '2026-04-15', correct: 50, total: 60, durationSecs: 600, newWords: 8 }]);
  mockApi.getHealth.mockResolvedValue(mockHealth('healthy'));
  mockApi.checkUpdate.mockResolvedValue({ currentVersion: '1.0.0', latestVersion: '1.0.0', hasUpdate: false, releaseUrl: null });
}

describe('DashboardPage — status & update banner', () => {
  beforeEach(() => vi.clearAllMocks());

  it('shows degraded status visual', async () => {
    primeBase();
    mockApi.getHealth.mockResolvedValue(mockHealth('degraded'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('性能降级')).toBeInTheDocument());
  });

  it('shows down status visual', async () => {
    primeBase();
    mockApi.getHealth.mockResolvedValue(mockHealth('down'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('服务异常')).toBeInTheDocument());
  });

  it('shows update banner when hasUpdate', async () => {
    primeBase();
    mockApi.checkUpdate.mockResolvedValue({ currentVersion: '1.0.0', latestVersion: '1.1.0', hasUpdate: true, releaseUrl: 'https://x.com' });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/新版本 1.1.0/)).toBeInTheDocument());
  });

  it('changes days window via picker', async () => {
    primeBase();
    await renderPage();
    await waitFor(() => expect(screen.getByText('全局概览')).toBeInTheDocument());
    // WindowPicker 升级为 radiogroup；按 14天 radio 触发切换
    const btn14 = screen.getAllByRole('radio').find((b) => b.textContent === '14天');
    expect(btn14).toBeDefined();
    fireEvent.click(btn14!);
    await waitFor(() => expect(mockApi.getStudyOverview).toHaveBeenCalledWith(14));
  });

  it('shows fallback empty when all resources fail', async () => {
    mockApi.getStats.mockRejectedValue(new Error('a'));
    mockApi.getEngagement.mockRejectedValue(new Error('b'));
    mockApi.getStudyOverview.mockRejectedValue(new Error('c'));
    mockApi.getDailyActiveUsers.mockRejectedValue(new Error('d'));
    mockApi.getDailyRecords.mockRejectedValue(new Error('e'));
    mockApi.getHealth.mockRejectedValue(new Error('f'));
    mockApi.checkUpdate.mockResolvedValue({ currentVersion: '0', latestVersion: '0', hasUpdate: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText('加载失败')).toBeInTheDocument());
  });
});
