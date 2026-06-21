import { onCleanup, createEffect, Show, type JSX } from 'solid-js';
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

  // 创建或重建 instance（dispose + init），用于 theme 切换；返回新建的实例（或 null）。
  const createInstance = (): echarts.ECharts | null => {
    if (!containerRef) return null;
    const theme = themeStore.effective() === 'dark' ? 'dark' : undefined;
    instance = echarts.init(containerRef, theme);
    return instance;
  };

  // 容器挂载时（含初始为空、传了 empty 回退、数据后到才挂载的场景）惰性初始化，
  // 并立即应用当前 option，避免「先 render empty → onMount 时无 container → 永不初始化」。
  const initOnMount = (el: HTMLDivElement) => {
    // div 重新挂载（Show 从 empty 切回）到新节点时，旧 instance 绑定的是已移除的 DOM，
    // 需先销毁再以新容器重建，避免渲染到游离节点。
    if (instance && containerRef !== el) {
      resizeOb?.disconnect();
      resizeOb = null;
      instance.dispose();
      instance = null;
    }
    containerRef = el;
    if (instance) return; // 幂等：已初始化则跳过
    const inst = createInstance();
    inst?.setOption(withDefaults(props.option()));
    // 容器尺寸为 0（父级 display:none）时跳过 resize 避免 ECharts warning，
    // 后续 ResizeObserver 在变为可见时会自动追加 resize。
    resizeOb = new ResizeObserver((entries) => {
      if (entries[0]?.contentRect.width > 0) instance?.resize();
    });
    resizeOb.observe(el);
  };

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

  // option 变化：setOption + replaceMerge:['series']。
  // 默认 merge 语义在新 series 数组变短时（如时间窗口缩小、算法集减少）不会清除多余的旧
  // series，残留为 ghost 曲线/柱。用 replaceMerge 整体替换 series，移除的 series 随之清空。
  createEffect(() => {
    const opt = props.option();
    if (!instance) return;
    instance.setOption(withDefaults(opt), { replaceMerge: ['series'] });
  });

  onCleanup(() => {
    resizeOb?.disconnect();
    instance?.dispose();
    instance = null;
  });

  return (
    <Show when={hasSeriesData(props.option()) || !props.empty} fallback={props.empty}>
      <div
        ref={initOnMount}
        class={props.class}
        style={{ height: props.height ?? '320px' }}
      />
    </Show>
  );
}
