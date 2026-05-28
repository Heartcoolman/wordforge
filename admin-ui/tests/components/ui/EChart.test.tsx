import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@solidjs/testing-library';

const setOption = vi.fn();
const dispose = vi.fn();
const resize = vi.fn();
let initInstance = { setOption, dispose, resize };

vi.mock('echarts/core', async () => {
  return {
    init: vi.fn(() => initInstance),
    use: vi.fn(),
  };
});
vi.mock('echarts/charts', () => ({
  LineChart: {}, BarChart: {}, PieChart: {}, ScatterChart: {},
}));
vi.mock('echarts/components', () => ({
  GridComponent: {}, TooltipComponent: {}, LegendComponent: {},
}));
vi.mock('echarts/renderers', () => ({ CanvasRenderer: {} }));

beforeEach(() => {
  setOption.mockClear();
  dispose.mockClear();
  resize.mockClear();
  initInstance = { setOption, dispose, resize };
});

import { EChart } from '@/components/ui/EChart';

describe('EChart', () => {
  it('initializes chart and calls setOption with returned option', () => {
    const opt = () => ({ title: { text: 'x' } });
    render(() => <EChart option={opt} />);
    expect(setOption).toHaveBeenCalled();
  });

  it('uses default height when not provided', () => {
    const { container } = render(() => <EChart option={() => ({})} />);
    const div = container.querySelector('div')!;
    expect(div.style.height).toBe('320px');
  });

  it('uses provided custom height', () => {
    const { container } = render(() => <EChart option={() => ({})} height="500px" />);
    const div = container.querySelector('div')!;
    expect(div.style.height).toBe('500px');
  });

  it('applies class when provided', () => {
    const { container } = render(() => <EChart option={() => ({})} class="my-chart" />);
    const div = container.querySelector('div')!;
    expect(div.className).toContain('my-chart');
  });

  it('disposes instance on cleanup', () => {
    const { unmount } = render(() => <EChart option={() => ({})} />);
    unmount();
    expect(dispose).toHaveBeenCalled();
  });

  it('does not subscribe to global DOM mutation events', () => {
    const observer = vi.spyOn(window, 'MutationObserver');
    render(() => <EChart option={() => ({})} />);
    expect(observer).not.toHaveBeenCalled();
    observer.mockRestore();
  });

  it('renders empty fallback when series data are empty', () => {
    const { container, getByText } = render(() => (
      <EChart
        option={() => ({ series: [{ type: 'bar', data: [] }] })}
        empty={<span>暂无</span>}
      />
    ));
    expect(getByText('暂无')).toBeInTheDocument();
    expect(container.querySelector('div[style]')).toBeNull();
  });

  it('renders chart container when series has data even with empty prop', () => {
    const { container } = render(() => (
      <EChart
        option={() => ({ series: [{ type: 'bar', data: [1, 2, 3] }] })}
        empty={<span>暂无</span>}
      />
    ));
    expect(container.querySelector('div[style]')).not.toBeNull();
  });
});
