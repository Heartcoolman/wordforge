import { onMount, onCleanup, createEffect, Show, type JSX } from 'solid-js';
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
  /** 当 series 数据均为空时渲染的回退节点（不传则照常 render 图表） */
  empty?: JSX.Element;
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

function hasSeriesData(option: EChartsOption): boolean {
  const series = option.series;
  if (!series) return false;
  const list = Array.isArray(series) ? series : [series];
  return list.some((s) => {
    const data = (s as { data?: unknown[] }).data;
    return Array.isArray(data) && data.length > 0;
  });
}

export function EChart(props: EChartProps) {
  let containerRef: HTMLDivElement | undefined;
  let instance: echarts.ECharts | null = null;
  let resizeOb: ResizeObserver | null = null;

  // 创建或重建 instance（dispose + init），用于 theme 切换
  const createInstance = () => {
    if (!containerRef) return;
    const theme = themeStore.effective() === 'dark' ? 'dark' : undefined;
    instance = echarts.init(containerRef, theme);
  };

  onMount(() => {
    if (!containerRef) return;
    createInstance();
    // 容器尺寸为 0（父级 display:none）时跳过 resize 避免 ECharts warning，
    // 后续 ResizeObserver 在变为可见时会自动追加 resize。
    resizeOb = new ResizeObserver((entries) => {
      if (entries[0]?.contentRect.width > 0) instance?.resize();
    });
    resizeOb.observe(containerRef);
  });

  // theme 切换：销毁实例重建以保证主题完全应用（避免 setOption notMerge 丢字段）
  createEffect(() => {
    const _theme = themeStore.effective();
    if (instance) {
      const last = props.option();
      instance.dispose();
      createInstance();
      instance?.setOption(withDefaults(last));
    }
    // 引用一下 _theme 让 effect 跟踪
    void _theme;
  });

  // option 变化：仅 setOption，merge 模式
  createEffect(() => {
    const opt = props.option();
    if (!instance) return;
    instance.setOption(withDefaults(opt));
  });

  onCleanup(() => {
    resizeOb?.disconnect();
    instance?.dispose();
    instance = null;
  });

  return (
    <Show when={hasSeriesData(props.option()) || !props.empty} fallback={props.empty}>
      <div
        ref={(el) => { containerRef = el; }}
        class={props.class}
        style={{ height: props.height ?? '320px' }}
      />
    </Show>
  );
}
