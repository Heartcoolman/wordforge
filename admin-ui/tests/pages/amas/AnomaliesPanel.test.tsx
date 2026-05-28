import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: { amasAnomaliesOverview: vi.fn() },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const emptyOverview = {
  totalEvents: 0, anomalyCount: 0, violationCount: 0,
  coldStartExplore: 0, coldStartExploit: 0,
  byDay: [], topViolationFields: [],
};
const fullOverview = {
  totalEvents: 100, anomalyCount: 5, violationCount: 3,
  coldStartExplore: 1, coldStartExploit: 2,
  byDay: [{ date: '2026-04-15', total: 50, anomalies: 2, violations: 1 }],
  topViolationFields: [{ field: 'memoryModel.w[0]', count: 2 }],
};

describe('AnomaliesPanel', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPanel() {
    const { AnomaliesPanel } = await import('@/pages/amas/AnomaliesPanel');
    return renderWithProviders(() => <AnomaliesPanel />);
  }

  it('renders heading and window picker', async () => {
    mockApi.amasAnomaliesOverview.mockResolvedValue(emptyOverview);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('异常 / 不变量违反')).toBeInTheDocument());
    expect(screen.getByText('7 天')).toBeInTheDocument();
  });

  it('shows empty state when no events', async () => {
    mockApi.amasAnomaliesOverview.mockResolvedValue(emptyOverview);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('时间窗口内无事件')).toBeInTheDocument());
  });

  it('shows charts and stats when populated', async () => {
    mockApi.amasAnomaliesOverview.mockResolvedValue(fullOverview);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('按天')).toBeInTheDocument());
    expect(screen.getByText('Top 违反字段')).toBeInTheDocument();
  });

  it('switches days window via picker', async () => {
    mockApi.amasAnomaliesOverview.mockResolvedValue(emptyOverview);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('14 天')).toBeInTheDocument());
    fireEvent.click(screen.getByText('14 天'));
    await waitFor(() => expect(mockApi.amasAnomaliesOverview).toHaveBeenCalledWith(14));
    fireEvent.click(screen.getByText('30 天'));
    await waitFor(() => expect(mockApi.amasAnomaliesOverview).toHaveBeenCalledWith(30));
  });
});
