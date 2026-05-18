import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: { amasMetricsTimeseries: vi.fn() },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('MetricsDashboard', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPanel() {
    const { MetricsDashboard } = await import('@/pages/admin/amas/MetricsDashboard');
    return renderWithProviders(() => <MetricsDashboard />);
  }

  it('renders heading and empty state when no data', async () => {
    mockApi.amasMetricsTimeseries.mockResolvedValue([]);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('算法延迟 / 错误率')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('暂无聚合数据')).toBeInTheDocument());
  });

  it('renders chart when data present', async () => {
    mockApi.amasMetricsTimeseries.mockResolvedValue([
      { date: '2026-04-15', algorithm: 'Heuristic', avgLatencyUs: 1500, errorCount: 0 },
      { date: '2026-04-16', algorithm: 'MDM', avgLatencyUs: 2000, errorCount: 1 },
    ]);
    await renderPanel();
    await waitFor(() => expect(screen.getByTestId('chart')).toBeInTheDocument());
  });

  it('uses fallback color for unknown algorithm', async () => {
    mockApi.amasMetricsTimeseries.mockResolvedValue([
      { date: '2026-04-15', algorithm: 'Unknown', avgLatencyUs: 100, errorCount: 0 },
    ]);
    await renderPanel();
    await waitFor(() => expect(screen.getByTestId('chart')).toBeInTheDocument());
  });

  it('changes days window via buttons', async () => {
    mockApi.amasMetricsTimeseries.mockResolvedValue([]);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('14 天')).toBeInTheDocument());
    fireEvent.click(screen.getByText('14 天'));
    await waitFor(() => expect(mockApi.amasMetricsTimeseries).toHaveBeenCalledWith(14));
    fireEvent.click(screen.getByText('30 天'));
    await waitFor(() => expect(mockApi.amasMetricsTimeseries).toHaveBeenCalledWith(30));
  });
});
