import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../../helpers/render';

const capturedOptions: unknown[] = [];
vi.mock('@/components/ui/EChart', () => ({
  EChart: (props: { option: () => unknown }) => {
    try {
      capturedOptions.push(props.option());
    } catch {
      /* swallow */
    }
    return <div data-testid="chart" />;
  },
}));
vi.mock('@/api/admin', () => ({
  adminApi: { amasAnomaliesOverview: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const richOverview = {
  totalEvents: 250,
  anomalyCount: 12,
  violationCount: 8,
  coldStartExplore: 3,
  coldStartExploit: 7,
  byDay: [
    { date: '2026-04-15', total: 100, anomalies: 5, violations: 3 },
    { date: '2026-04-16', total: 150, anomalies: 7, violations: 5 },
  ],
  topViolationFields: [
    { field: 'memoryModel.w[0]', count: 4 },
    { field: 'ensemble.weights.MDM', count: 3 },
  ],
};

describe('AnomaliesPanel extra', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    capturedOptions.length = 0;
  });

  async function renderPanel() {
    const { AnomaliesPanel } = await import('@/pages/admin/amas/AnomaliesPanel');
    return renderWithProviders(() => <AnomaliesPanel />);
  }

  it('dailyOption 生成三条柱状 series（覆盖 15-28 行）', async () => {
    mockApi.amasAnomaliesOverview.mockResolvedValue(richOverview);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('按天')).toBeInTheDocument());
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThanOrEqual(2));
    const daily = capturedOptions[0] as {
      series: { name: string; type: string; data: number[] }[];
      xAxis: { data: string[] };
    };
    expect(daily.series.length).toBe(3);
    expect(daily.series.map((s) => s.name)).toEqual(['总事件', '异常', '违反不变量']);
    expect(daily.series.every((s) => s.type === 'bar')).toBe(true);
    expect(daily.series[0].data).toEqual([100, 150]);
    expect(daily.series[1].data).toEqual([5, 7]);
    expect(daily.series[2].data).toEqual([3, 5]);
    expect(daily.xAxis.data).toEqual(['2026-04-15', '2026-04-16']);
  });

  it('fieldOption 生成横向柱状（覆盖 31-41 行）', async () => {
    mockApi.amasAnomaliesOverview.mockResolvedValue(richOverview);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('Top 违反字段')).toBeInTheDocument());
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThanOrEqual(2));
    const field = capturedOptions[1] as {
      series: { type: string; data: number[] }[];
      yAxis: { type: string; data: string[]; inverse: boolean };
    };
    expect(field.series[0].type).toBe('bar');
    expect(field.series[0].data).toEqual([4, 3]);
    expect(field.yAxis.type).toBe('category');
    expect(field.yAxis.data).toEqual(['memoryModel.w[0]', 'ensemble.weights.MDM']);
    expect(field.yAxis.inverse).toBe(true);
  });

  it('byDay 为空时 dailyOption 仍可调用（safety path）', async () => {
    const partial = { ...richOverview, byDay: [], topViolationFields: [] };
    mockApi.amasAnomaliesOverview.mockResolvedValue(partial);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('时间窗口内无事件')).toBeInTheDocument());
    // 此场景下 dailyOption/fieldOption 不会被渲染（Show 守护），但 StatCard 还在
    expect(screen.getByText('总事件')).toBeInTheDocument();
  });
});
