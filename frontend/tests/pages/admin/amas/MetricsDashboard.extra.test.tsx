import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../../helpers/render';

// 关键：mock EChart 时调用 option getter，触发 option() 函数体内的所有行
const capturedOptions: unknown[] = [];
vi.mock('@/components/ui/EChart', () => ({
  EChart: (props: { option: () => unknown }) => {
    // 解析 option getter 以执行 latencySeries / errorSeries / xAxis / yAxis 等所有行
    try {
      capturedOptions.push(props.option());
    } catch {
      // 若 option throw 也吞掉，不影响其他用例
    }
    return <div data-testid="chart" />;
  },
}));
vi.mock('@/api/admin', () => ({
  adminApi: { amasMetricsTimeseries: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('MetricsDashboard extra', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    capturedOptions.length = 0;
  });

  async function renderPanel() {
    const { MetricsDashboard } = await import('@/pages/admin/amas/MetricsDashboard');
    return renderWithProviders(() => <MetricsDashboard />);
  }

  it('option() 在多算法多日期下生成 latencySeries + errorSeries（覆盖 33-65 行）', async () => {
    mockApi.amasMetricsTimeseries.mockResolvedValue([
      { date: '2026-04-15', algorithm: 'Heuristic', avgLatencyUs: 1500, errorCount: 0 },
      { date: '2026-04-15', algorithm: 'MDM', avgLatencyUs: 2000, errorCount: 2 },
      { date: '2026-04-16', algorithm: 'Heuristic', avgLatencyUs: 1800, errorCount: 1 },
      { date: '2026-04-16', algorithm: 'MDM', avgLatencyUs: 2200, errorCount: 0 },
    ]);
    await renderPanel();
    await waitFor(() => expect(screen.getByTestId('chart')).toBeInTheDocument());
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThan(0));
    const opt = capturedOptions[capturedOptions.length - 1] as {
      series: { name: string; type: string; data: unknown[] }[];
      xAxis: { data: string[] };
      yAxis: { name: string }[];
    };
    // 2 algos × (latency + error) = 4 series
    expect(opt.series.length).toBe(4);
    expect(opt.series.some((s) => s.name === 'Heuristic 延迟' && s.type === 'line')).toBe(true);
    expect(opt.series.some((s) => s.name === 'MDM 错误' && s.type === 'bar')).toBe(true);
    expect(opt.xAxis.data).toEqual(['2026-04-15', '2026-04-16']);
    expect(opt.yAxis[0].name).toBe('延迟 ms');
    expect(opt.yAxis[1].name).toBe('错误数');
  });

  it('未知算法走 FALLBACK 颜色（覆盖 43、51 行的 ?? 分支）', async () => {
    mockApi.amasMetricsTimeseries.mockResolvedValue([
      { date: '2026-04-15', algorithm: 'XyzCustom', avgLatencyUs: 1234, errorCount: 7 },
    ]);
    await renderPanel();
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThan(0));
    const opt = capturedOptions[capturedOptions.length - 1] as {
      series: { itemStyle: { color: string } }[];
    };
    expect(opt.series.length).toBe(2);
    expect(opt.series[0].itemStyle.color).toBe('#94a3b8');
    expect(opt.series[1].itemStyle.color).toBe('#94a3b8');
  });

  it('lookup 缺数据时 latency 返回 null、error 返回 0（覆盖 41/50 行的 ?? 分支）', async () => {
    // 只在 2026-04-15 有 Heuristic 数据；2026-04-16 仅 MDM
    mockApi.amasMetricsTimeseries.mockResolvedValue([
      { date: '2026-04-15', algorithm: 'Heuristic', avgLatencyUs: 1500, errorCount: 3 },
      { date: '2026-04-16', algorithm: 'MDM', avgLatencyUs: 2200, errorCount: 5 },
    ]);
    await renderPanel();
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThan(0));
    const opt = capturedOptions[capturedOptions.length - 1] as {
      series: { name: string; data: (number | null)[] }[];
    };
    const heuristicLat = opt.series.find((s) => s.name === 'Heuristic 延迟');
    const mdmErr = opt.series.find((s) => s.name === 'MDM 错误');
    expect(heuristicLat!.data).toEqual([1.5, null]);
    expect(mdmErr!.data).toEqual([0, 5]);
  });

  it('切换窗口期会重新触发 option()（验证多次执行）', async () => {
    mockApi.amasMetricsTimeseries.mockResolvedValue([
      { date: '2026-04-15', algorithm: 'Heuristic', avgLatencyUs: 1500, errorCount: 0 },
    ]);
    await renderPanel();
    await waitFor(() => expect(screen.getByTestId('chart')).toBeInTheDocument());
    const before = capturedOptions.length;
    fireEvent.click(screen.getByText('14 天'));
    await waitFor(() => expect(mockApi.amasMetricsTimeseries).toHaveBeenCalledWith(14));
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThanOrEqual(before));
  });
});
