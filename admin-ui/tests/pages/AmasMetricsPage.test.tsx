import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasMetricsTimeseries: vi.fn().mockResolvedValue([]),
    amasAnomaliesOverview: vi.fn().mockResolvedValue({
      totalEvents: 0, anomalyCount: 0, violationCount: 0, coldStartExplore: 0, coldStartExploit: 0,
      byDay: [], topViolationFields: [],
    }),
    amasListVersions: vi.fn().mockResolvedValue([]),
    amasCompareVersions: vi.fn(),
    amasUserStateDistribution: vi.fn().mockResolvedValue({
      attention: { field: 'attention', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
      fatigue: { field: 'fatigue', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
      motivation: { field: 'motivation', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
      confidence: { field: 'confidence', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
      coldStartExplore: 0,
      coldStartExploit: 0,
    }),
  },
}));
vi.mock('@/components/ui/EChart', () => ({
  EChart: () => <div data-testid="chart" />,
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('AmasMetricsPage', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/AmasMetricsPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders all four tabs', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByRole('tab', { name: '算法延迟 / 错误率' })).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: '版本对比（预测 / 留存）' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '异常 / 不变量' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '用户状态分布' })).toBeInTheDocument();
  });

  it('shows metrics panel by default', async () => {
    await renderPage();
    await waitFor(() => expect(screen.getByRole('heading', { name: '算法延迟 / 错误率' })).toBeInTheDocument());
  });

  it('switches to compare tab', async () => {
    await renderPage();
    fireEvent.click(screen.getByRole('tab', { name: '版本对比（预测 / 留存）' }));
    await waitFor(() => expect(screen.getByRole('heading', { name: '版本对比' })).toBeInTheDocument());
  });

  it('switches to anomalies tab', async () => {
    await renderPage();
    fireEvent.click(screen.getByRole('tab', { name: '异常 / 不变量' }));
    await waitFor(() => expect(screen.getByRole('heading', { name: '异常 / 不变量违反' })).toBeInTheDocument());
  });

  it('switches to user-state tab', async () => {
    await renderPage();
    fireEvent.click(screen.getByRole('tab', { name: '用户状态分布' }));
    await waitFor(() => expect(screen.getByRole('heading', { name: '用户状态分布' })).toBeInTheDocument());
  });
});
