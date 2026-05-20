import { createResource, createSignal, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { StatCard } from '@/components/ui/StatCard';
import { adminApi } from '@/api/admin';
import { chartColor } from '@/lib/chartTheme';

/** C3: Anomaly / Invariant Violation 表盘 */
export function AnomaliesPanel() {
  const [days, setDays] = createSignal<7 | 14 | 30>(7);
  const [overview] = createResource(days, async (d) => adminApi.amasAnomaliesOverview(d));

  const dailyOption = (): import('echarts').EChartsOption => {
    const data = overview()?.byDay ?? [];
    return {
      grid: { left: 50, right: 20, top: 40, bottom: 30 },
      legend: { top: 0, data: ['总事件', '异常', '违反不变量'] },
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
      xAxis: { type: 'category', data: data.map((d) => d.date) },
      yAxis: { type: 'value', minInterval: 1 },
      series: [
        { name: '总事件', type: 'bar', data: data.map((d) => d.total), itemStyle: { color: chartColor('muted'), opacity: 0.6 } },
        { name: '异常', type: 'bar', data: data.map((d) => d.anomalies), itemStyle: { color: chartColor('error') } },
        { name: '违反不变量', type: 'bar', data: data.map((d) => d.violations), itemStyle: { color: chartColor('warning') } },
      ],
    };
  };

  const fieldOption = (): import('echarts').EChartsOption => {
    const fields = overview()?.topViolationFields ?? [];
    return {
      grid: { left: 100, right: 20, top: 20, bottom: 30 },
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
      xAxis: { type: 'value', minInterval: 1 },
      yAxis: { type: 'category', data: fields.map((f) => f.field), inverse: true },
      series: [
        { type: 'bar', data: fields.map((f) => f.count), itemStyle: { color: chartColor('warning') }, barWidth: 16 },
      ],
    };
  };

  return (
    <div class="space-y-3">
      <Card variant="elevated">
        <div class="flex items-baseline justify-between mb-3">
          <h2 class="text-lg font-semibold text-content">异常 / 不变量违反</h2>
          <div class="flex items-center gap-1.5" role="group" aria-label="选择时间窗口">
            <For each={[7, 14, 30] as const}>
              {(n) => (
                <button
                  type="button"
                  onClick={() => setDays(n)}
                  aria-pressed={days() === n}
                  aria-label={`使用 ${n} 天窗口`}
                  class={`focus-ring-soft px-2.5 py-1 text-xs rounded-md transition-colors ${
                    days() === n ? 'bg-accent text-accent-content' : 'bg-surface-secondary text-content-secondary hover:text-content'
                  }`}
                >
                  {n} 天
                </button>
              )}
            </For>
          </div>
        </div>
        <Show when={!overview.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
          {(_) => {
            const ov = () => overview()!;
            return (
              <div class="space-y-4">
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
                  <StatCard
                    title="总事件"
                    value={ov().totalEvents}
                    color="info"
                    icon="M4 6h16M4 10h16M4 14h16M4 18h16"
                  />
                  <StatCard
                    title="异常次数"
                    value={ov().anomalyCount}
                    color="error"
                    icon="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.034 16.5c-.77.833.192 2.5 1.732 2.5z"
                  />
                  <StatCard
                    title="违反不变量"
                    value={ov().violationCount}
                    color="warning"
                    icon="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                  <StatCard
                    title="冷启动 explore/exploit"
                    value={`${ov().coldStartExplore}/${ov().coldStartExploit}`}
                    color="success"
                    icon="M13 10V3L4 14h7v7l9-11h-7z"
                  />
                </div>

                <Show when={ov().byDay.length > 0} fallback={<Empty title="时间窗口内无事件" description="尝试拉长窗口或检查是否落库" />}>
                  <div>
                    <h3 class="text-sm font-medium text-content-secondary mb-2">按天</h3>
                    <EChart option={dailyOption} height="280px" />
                  </div>
                </Show>

                <Show when={ov().topViolationFields.length > 0}>
                  <div>
                    <h3 class="text-sm font-medium text-content-secondary mb-2">Top 违反字段</h3>
                    <EChart option={fieldOption} height={`${Math.max(160, ov().topViolationFields.length * 28)}px`} />
                  </div>
                </Show>
              </div>
            );
          }}
        </Show>
      </Card>
    </div>
  );
}
