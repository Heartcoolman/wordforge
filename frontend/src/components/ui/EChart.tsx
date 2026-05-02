import { onMount, onCleanup, createEffect } from 'solid-js';
import * as echarts from 'echarts/core';
import { LineChart, BarChart, PieChart, ScatterChart } from 'echarts/charts';
import { GridComponent, TooltipComponent, LegendComponent } from 'echarts/components';
import { CanvasRenderer } from 'echarts/renderers';
import type { EChartsOption } from 'echarts';

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
  let mutationOb: MutationObserver | null = null;

  onMount(() => {
    if (!containerRef) return;

    instance = echarts.init(containerRef);
    instance.setOption(props.option());

    resizeOb = new ResizeObserver(() => instance?.resize());
    resizeOb.observe(containerRef);

    mutationOb = new MutationObserver(() => {
      if (instance) instance.setOption(props.option(), { notMerge: true });
    });
    mutationOb.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });
  });

  createEffect(() => {
    instance?.setOption(props.option(), { notMerge: true });
  });

  onCleanup(() => {
    mutationOb?.disconnect();
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
