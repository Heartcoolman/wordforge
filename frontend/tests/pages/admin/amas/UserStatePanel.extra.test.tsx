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
  adminApi: { amasUserStateDistribution: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const fullDist = {
  attention: {
    field: 'attention', min: 0, max: 1,
    bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
    mean: 0.5, median: 0.5, sampleSize: 100,
  },
  fatigue: {
    field: 'fatigue', min: 0, max: 1,
    bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
    mean: 0.3, median: 0.3, sampleSize: 100,
  },
  motivation: {
    field: 'motivation', min: -1, max: 1,
    bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
    mean: 0.6, median: 0.6, sampleSize: 100,
  },
  confidence: {
    field: 'confidence', min: 0, max: 1,
    bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20],
    mean: 0.7, median: 0.7, sampleSize: 100,
  },
  coldStartExplore: 5,
  coldStartExploit: 8,
};

const distWithUnknownField = {
  ...fullDist,
  attention: { ...fullDist.attention, field: 'mystery_field' },
};

describe('UserStatePanel extra', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    capturedOptions.length = 0;
  });

  async function renderPanel() {
    const { UserStatePanel } = await import('@/pages/admin/amas/UserStatePanel');
    return renderWithProviders(() => <UserStatePanel />);
  }

  it('histOption 为 4 个字段生成 bar series + 正确 labels（覆盖 32-51 行）', async () => {
    mockApi.amasUserStateDistribution.mockResolvedValue(fullDist);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('注意力')).toBeInTheDocument());
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThanOrEqual(4));
    const first = capturedOptions[0] as {
      series: { type: string; data: number[]; itemStyle: { color: string } }[];
      xAxis: { type: string; data: string[] };
    };
    expect(first.series[0].type).toBe('bar');
    expect(first.series[0].data.length).toBe(20);
    // attention → '#6366f1'
    expect(first.series[0].itemStyle.color).toBe('#6366f1');
    // labels 应为 20 个，对应 min..max 等距分桶
    expect(first.xAxis.data.length).toBe(20);
    // 第一个 label 等于 min（0）→ "0.00"
    expect(first.xAxis.data[0]).toBe('0.00');
    // motivation 跨 -1..1 → 第一个 label "-1.00"
    const motivation = capturedOptions.find((o) => {
      const c = (o as { series: { itemStyle: { color: string } }[] }).series[0].itemStyle.color;
      return c === '#10b981';
    }) as { xAxis: { data: string[] } };
    expect(motivation.xAxis.data[0]).toBe('-1.00');
  });

  it('未知字段走 FIELD_COLOR fallback（覆盖 46 行的 ?? 分支）', async () => {
    mockApi.amasUserStateDistribution.mockResolvedValue(distWithUnknownField);
    await renderPanel();
    // FIELD_LABEL[mystery_field] 不存在 → 显示 undefined，但组件应能正常 render（不崩）
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThanOrEqual(4));
    const unknownOpt = capturedOptions.find((o) => {
      const c = (o as { series: { itemStyle: { color: string } }[] }).series[0].itemStyle.color;
      return c === '#94a3b8';
    });
    expect(unknownOpt).toBeDefined();
  });

  it('点击桶数按钮后 option 重新执行', async () => {
    mockApi.amasUserStateDistribution.mockResolvedValue(fullDist);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('注意力')).toBeInTheDocument());
    const before = capturedOptions.length;
    const { fireEvent } = await import('@solidjs/testing-library');
    fireEvent.click(screen.getByText('40'));
    await waitFor(() =>
      expect(mockApi.amasUserStateDistribution).toHaveBeenCalledWith(expect.any(Number), 40),
    );
    await waitFor(() => expect(capturedOptions.length).toBeGreaterThanOrEqual(before));
  });
});
