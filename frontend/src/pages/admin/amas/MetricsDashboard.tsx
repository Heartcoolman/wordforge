import { createResource, createMemo, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { adminApi, type AmasMetricsTimeseriesPoint } from '@/api/admin';
import { algorithmColor } from '@/lib/chartTheme';

/** C1: 算法延迟 / 错误率时间序列。双 Y 轴 — 左 latency μs，右 error 数 */
export function MetricsDashboard() {
  const [days, setDays] = createSignal<7 | 14 | 30>(7);
  const [series] = createResource(days, async (d) => adminApi.amasMetricsTimeseries(d));

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
      // 12 项 legend（6 算法 × 2 维度）+ 双 y 轴 name，top 留 96px 给两行 legend，避免与 axis name 重叠
      grid: { left: 64, right: 64, top: 96, bottom: 40 },
      legend: {
        top: 4,
        type: 'plain',          // 允许自动换行，比 scroll 模式更直观
        itemGap: 14,
        itemWidth: 14,
        textStyle: { fontSize: 12 },
      },
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
    <div class="space-y-3">
      <Card variant="elevated">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="text-lg font-semibold text-content">算法延迟 / 错误率</h2>
          <div class="flex items-center gap-1.5">
            <For each={[7, 14, 30] as const}>
              {(n) => (
                <button
                  type="button"
                  onClick={() => setDays(n)}
                  class={`px-2.5 py-1 text-xs rounded-md transition-colors ${
                    days() === n ? 'bg-accent text-white' : 'bg-surface-secondary text-content-secondary hover:text-content'
                  }`}
                >
                  {n} 天
                </button>
              )}
            </For>
          </div>
        </div>
        <Show when={!series.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
          <Show when={grouped().dates.length > 0} fallback={<Empty title="暂无聚合数据" description="algorithm_metrics_daily 表当前为空" />}>
            <EChart option={option} height="440px" />
          </Show>
        </Show>
      </Card>
    </div>
  );
}
