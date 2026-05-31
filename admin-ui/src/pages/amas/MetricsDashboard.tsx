import { createResource, createMemo, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { adminApi, type AmasMetricsTimeseriesPoint } from '@/api/admin';
import { algorithmColor } from '@/lib/chartTheme';
import { KpiCards } from './KpiCards';
import { AlgorithmDonut } from './AlgorithmDonut';
import { StageSwitchPanel } from './StageSwitchPanel';
import { EloScatterPanel } from './EloScatterPanel';
import { MdmHeatmapPanel } from './MdmHeatmapPanel';
import { FatigueTimeseriesPanel } from './FatigueTimeseriesPanel';
import { DecisionHistogramPanel } from './DecisionHistogramPanel';
import { PanelBoundary } from './PanelBoundary';

/** metrics tab：KPI + 算法分布甜甜圈 / 阶段切换 / ELO 散点 / MDM 热图 / 疲劳时序 / 决策直方图，
 * 末尾保留算法延迟·错误率双轴时序（与 tab 名一致）。全部真实数据，days 时间窗联动。 */
export function MetricsDashboard(props: { days: () => number }) {
  const [series] = createResource(props.days, async (d) => adminApi.amasMetricsTimeseries(d));

  const grouped = createMemo(() => {
    const data = series() ?? [];
    const dates = Array.from(new Set(data.map((p) => p.date))).sort();
    const algos = Array.from(new Set(data.map((p) => p.algorithm))).sort();
    const idx = new Map<string, AmasMetricsTimeseriesPoint>();
    for (const p of data) idx.set(`${p.date}|${p.algorithm}`, p);
    return { dates, algos, lookup: idx };
  });

  const option = (): import('echarts').EChartsOption => {
    const { dates, algos, lookup } = grouped();
    const latencySeries = algos.map((algo) => ({
      name: `${algo} 延迟`,
      type: 'line' as const,
      smooth: true,
      yAxisIndex: 0,
      data: dates.map((d) => {
        const p = lookup.get(`${d}|${algo}`);
        return p ? Number((p.avgLatencyUs / 1000).toFixed(3)) : null; // → ms
      }),
      itemStyle: { color: algorithmColor(algo) },
      lineStyle: { width: 2 },
    }));
    const errorSeries = algos.map((algo) => ({
      name: `${algo} 错误`,
      type: 'bar' as const,
      yAxisIndex: 1,
      data: dates.map((d) => lookup.get(`${d}|${algo}`)?.errorCount ?? 0),
      itemStyle: { color: algorithmColor(algo), opacity: 0.35 },
      barGap: '5%',
    }));
    return {
      grid: { left: 64, right: 64, top: 96, bottom: 40 },
      legend: { top: 4, type: 'plain', itemGap: 14, itemWidth: 14, textStyle: { fontSize: 12 } },
      tooltip: { trigger: 'axis' },
      xAxis: { type: 'category', data: dates },
      yAxis: [
        { type: 'value', name: '延迟 ms', position: 'left', nameGap: 12, nameTextStyle: { fontSize: 11 } },
        { type: 'value', name: '错误数', position: 'right', minInterval: 1, nameGap: 12, nameTextStyle: { fontSize: 11 } },
      ],
      series: [...latencySeries, ...errorSeries],
    };
  };

  return (
    <div class="space-y-4">
      <PanelBoundary><KpiCards days={props.days} /></PanelBoundary>

      {/* 行 1：算法分布甜甜圈(7) + 阶段切换(5) */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
        <div class="lg:col-span-7"><PanelBoundary><AlgorithmDonut days={props.days} /></PanelBoundary></div>
        <div class="lg:col-span-5"><PanelBoundary><StageSwitchPanel /></PanelBoundary></div>
      </div>

      {/* 行 2：ELO 散点(7) + MDM 热图(5) */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
        <div class="lg:col-span-7"><PanelBoundary><EloScatterPanel /></PanelBoundary></div>
        <div class="lg:col-span-5"><PanelBoundary><MdmHeatmapPanel days={props.days} /></PanelBoundary></div>
      </div>

      {/* 行 3：疲劳时序(7) + 决策直方图(5) */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-4">
        <div class="lg:col-span-7"><PanelBoundary><FatigueTimeseriesPanel days={props.days} /></PanelBoundary></div>
        <div class="lg:col-span-5"><PanelBoundary><DecisionHistogramPanel days={props.days} /></PanelBoundary></div>
      </div>

      {/* 算法延迟 / 错误率双轴时序（对齐 tab 名） */}
      <Card variant="elevated" class="amas-panel">
        <div class="amas-panel-head"><h3>算法延迟 / 错误率</h3><span class="amas-sub">左轴延迟 ms · 右轴错误数</span></div>
        <Show when={!series.loading} fallback={<div class="min-h-[440px] flex items-center justify-center"><Spinner /></div>}>
          <Show when={grouped().dates.length > 0} fallback={<Empty title="暂无聚合数据" description="algorithm_metrics_daily 表当前为空" />}>
            <EChart option={option} height="440px" />
          </Show>
        </Show>
      </Card>
    </div>
  );
}
