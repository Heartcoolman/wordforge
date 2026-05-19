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

/**
 * 默认动画配置 — 与 index.css 的 duration-slow / ease-out-expo 节奏一致；
 * 业务方在 option 中显式声明同名字段时会覆盖此默认。
 */
const DEFAULT_ANIMATION: Pick<EChartsOption, 'animation' | 'animationDuration' | 'animationEasing' | 'animationDurationUpdate' | 'animationEasingUpdate'> = {
  animation: true,
  animationDuration: 600,
  animationEasing: 'cubicOut',
  animationDurationUpdate: 320,
  animationEasingUpdate: 'cubicInOut',
};

function withDefaults(option: EChartsOption): EChartsOption {
  // 合并：业务 option 字段优先
  return { ...DEFAULT_ANIMATION, ...option };
}

export function EChart(props: EChartProps) {
  let containerRef: HTMLDivElement | undefined;
  let instance: echarts.ECharts | null = null;
  let resizeOb: ResizeObserver | null = null;

  onMount(() => {
    if (!containerRef) return;

    instance = echarts.init(containerRef);
    instance.setOption(withDefaults(props.option()));

    resizeOb = new ResizeObserver(() => instance?.resize());
    resizeOb.observe(containerRef);

  });

  createEffect(() => {
    themeStore.effective();
    instance?.setOption(withDefaults(props.option()), { notMerge: true });
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
