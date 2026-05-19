import { onMount, onCleanup, createEffect } from 'solid-js';
import * as echarts from 'echarts/core';
import { LineChart, BarChart, PieChart, ScatterChart } from 'echarts/charts';
import { GridComponent, TooltipComponent, LegendComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import type { EChartsOption } from 'echarts';
import { themeStore } from '@/stores/theme';

echarts.use([
  CanvasRenderer,
  LineChart, BarChart, PieChart, ScatterChart,
  GridComponent, TooltipComponent, LegendComponent,
]);

interface EChartProps {
  option: () => EChartsOption;
  class?: string;
  height?: string;
}

export function EChart(props: EChartProps) {
  let containerRef: HTMLDivElement | undefined;
  let instance: echarts.ECharts | null = null;
  let resizeOb: ResizeObserver | null = null;

  onMount(() => {
    if (!containerRef) return;

    instance = echarts.init(containerRef);
    instance.setOption(props.option());

    resizeOb = new ResizeObserver(() => instance?.resize());
    resizeOb.observe(containerRef);

  });

  createEffect(() => {
    themeStore.effective();
    instance?.setOption(props.option(), { notMerge: true });
  });

  onCleanup(() => {
    resizeOb?.disconnect();
    instance?.dispose();
    instance = null;
  });

  return (
    <div
      ref={(el) => { containerRef = el; }}
      class={props.class}
      style={{ height: props.height ?? '320px' }}
    />
  );
}
