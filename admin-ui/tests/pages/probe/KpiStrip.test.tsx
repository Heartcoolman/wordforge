import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, render } from '@solidjs/testing-library';
import type { ProbeOverview } from '@/types/probeTelemetry';

const overview = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    overview: (...a: unknown[]) => overview(...a),
  },
}));

function mkOverview(over: Partial<ProbeOverview> = {}): ProbeOverview {
  return {
    generatedAt: '2026-05-29T12:00:00Z',
    activeProbes: { value: 3, total: 4, note: '派生探针 note' },
    events24h: { value: 14700, deltaPct: 0.048, note: '事件 note' },
    queueBacklog: { value: 0, note: '队列 note' },
    collectErrorRate: { value: 0.0003, note: '错误率 note' },
    ...over,
  };
}

async function renderStrip(days = 7) {
  const { KpiStrip } = await import('@/pages/probe/KpiStrip');
  return render(() => <KpiStrip days={() => days} />);
}

describe('KpiStrip', () => {
  beforeEach(() => vi.clearAllMocks());

  it('数据未到达时渲染 Spinner 占位，不渲染健康 banner', async () => {
    overview.mockReturnValue(new Promise(() => {}));
    const { container } = await renderStrip(7);
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument());
    expect(container.querySelector('.pb-health')).toBeNull();
  });

  it('按 days 请求 overview，全绿数据渲染正常 banner + 四张 KPI 卡', async () => {
    overview.mockResolvedValue(mkOverview());
    const { container } = await renderStrip(7);
    await waitFor(() => expect(screen.getByText('系统健康度正常')).toBeInTheDocument());
    expect(overview).toHaveBeenCalledWith(7);
    expect(container.querySelector('.pb-health')).toHaveClass('is-normal');
    expect(container.querySelector('.pb-health-text')).toHaveTextContent(
      '3/4 个数据源在收数据，近 7d 上报 14.7k 条，无任务积压，采集错误率 0.03%，整体运转良好。',
    );
    // 四张 KPI 卡 + 四个健康药丸
    expect(container.querySelectorAll('.pb-kpi-grid .pb-kpi')).toHaveLength(4);
    expect(container.querySelectorAll('.pb-kpi-status.is-normal')).toHaveLength(4);
    expect(screen.getByText('4 个数据源里 3 个在收数据')).toBeInTheDocument();
    expect(screen.getByText('数据正常上报中')).toBeInTheDocument();
    expect(screen.getByText('没有任务积压，处理及时')).toBeInTheDocument();
    expect(screen.getByText('每 100 条上报约 0.03 条出错')).toBeInTheDocument();
  });

  it('数值格式化：事件缩写、错误率两位小数、活跃探针进度条百分比', async () => {
    overview.mockResolvedValue(mkOverview({ collectErrorRate: { value: 0.0123 } }));
    const { container } = await renderStrip(7);
    await waitFor(() => expect(container.querySelector('.pb-kpi-grid .pb-kpi')).toBeTruthy());
    const cards = container.querySelectorAll('.pb-kpi-grid .pb-kpi');
    expect(cards[0].querySelector('.v')).toHaveTextContent('3/ 4');
    expect(cards[0].querySelector<HTMLElement>('.bar-mini span')!.style.width).toBe('75%');
    expect(cards[1].querySelector('.v')).toHaveTextContent('14.7k条');
    expect(cards[3].querySelector('.v')).toHaveTextContent('1.23%');
  });

  it('days=1 时窗口标签为 24h，其余为 Nd', async () => {
    overview.mockResolvedValue(mkOverview());
    const { container } = await renderStrip(1);
    await waitFor(() => expect(container.querySelector('.pb-kpi-grid .pb-kpi')).toBeTruthy());
    expect(container.querySelector('.pb-kpi-grid')).toHaveTextContent('24h 上报事件');
    expect(container.querySelector('.delta')).toHaveTextContent('vs 前 24h');
  });

  it('deltaPct > 0 → ▲ up；< 0 → ▼ down', async () => {
    overview.mockResolvedValue(mkOverview({ events24h: { value: 100, deltaPct: 0.048 } }));
    const { container, unmount } = await renderStrip(7);
    await waitFor(() => expect(container.querySelector('.delta')).toBeTruthy());
    expect(container.querySelector('.delta')).toHaveClass('up');
    expect(container.querySelector('.delta')).toHaveTextContent('▲ 4.8% vs 前 7d');
    unmount();

    overview.mockResolvedValue(mkOverview({ events24h: { value: 100, deltaPct: -0.12 } }));
    const second = await renderStrip(7);
    await waitFor(() => expect(second.container.querySelector('.delta')).toBeTruthy());
    expect(second.container.querySelector('.delta')).toHaveClass('down');
    expect(second.container.querySelector('.delta')).toHaveTextContent('▼ 12.0% vs 前 7d');
  });

  it('deltaPct 为 0 时不加涨跌 class', async () => {
    overview.mockResolvedValue(mkOverview({ events24h: { value: 100, deltaPct: 0 } }));
    const { container } = await renderStrip(7);
    await waitFor(() => expect(container.querySelector('.delta')).toBeTruthy());
    expect(container.querySelector('.delta')!.className.trim()).toBe('delta');
  });

  it('deltaPct 缺失 → 显示无可比基准提示', async () => {
    overview.mockResolvedValue(mkOverview({ events24h: { value: 100 } }));
    const { container } = await renderStrip(7);
    await waitFor(() => expect(screen.getByText('无前一时段可比基准')).toBeInTheDocument());
    expect(container.querySelector('.delta')).toBeNull();
  });

  it('activeProbes.total 缺失 → 不渲染分母，进度条为 0', async () => {
    overview.mockResolvedValue(mkOverview({ activeProbes: { value: 2 } }));
    const { container } = await renderStrip(7);
    await waitFor(() => expect(container.querySelector('.pb-kpi.is-primary')).toBeTruthy());
    const primary = container.querySelector('.pb-kpi.is-primary')!;
    expect(primary.querySelector('.unit')).toBeNull();
    expect(primary.querySelector<HTMLElement>('.bar-mini span')!.style.width).toBe('0%');
  });

  it('note 缺失时 TechTip 回落到默认技术口径文案', async () => {
    overview.mockResolvedValue(
      mkOverview({
        activeProbes: { value: 3, total: 4 },
        events24h: { value: 100, deltaPct: 0 },
        queueBacklog: { value: 0 },
        collectErrorRate: { value: 0 },
      }),
    );
    await renderStrip(7);
    await waitFor(() =>
      expect(screen.getByText('有数据的派生探针数 / 总定义数')).toBeInTheDocument(),
    );
    expect(screen.getByText('learning_records + telemetry_events')).toBeInTheDocument();
    expect(screen.getByText('probe_executions 未完成(completed_at IS NULL)')).toBeInTheDocument();
    expect(screen.getByText('有错误的事件数 / 事件总数')).toBeInTheDocument();
  });

  it('队列积压超阈值 → 整体异常 banner + 异常药丸 + hint', async () => {
    overview.mockResolvedValue(mkOverview({ queueBacklog: { value: 50 } }));
    const { container } = await renderStrip(7);
    await waitFor(() => expect(screen.getByText('系统健康度异常')).toBeInTheDocument());
    expect(container.querySelector('.pb-health')).toHaveClass('is-abnormal');
    expect(container.querySelector('.pb-health-text')).toHaveTextContent('任务积压 50 条，请尽快排查。');
    expect(container.querySelectorAll('.pb-kpi-status.is-abnormal')).toHaveLength(1);
    expect(screen.getByText('积压 50 条，可能处理跟不上')).toBeInTheDocument();
    expect(screen.getByText('需排查消费端 / 扩容')).toBeInTheDocument();
  });

  it('上报量为 0 → 整体注意 banner + 事件卡注意文案', async () => {
    overview.mockResolvedValue(mkOverview({ events24h: { value: 0 } }));
    const { container } = await renderStrip(7);
    await waitFor(() => expect(screen.getByText('系统健康度注意')).toBeInTheDocument());
    expect(container.querySelector('.pb-health')).toHaveClass('is-attention');
    expect(container.querySelector('.pb-health-text')).toHaveTextContent('近期无上报，建议关注趋势。');
    expect(screen.getByText('近期没有新数据，需确认是否断流')).toBeInTheDocument();
  });
});
