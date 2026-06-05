import { createResource, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { adminApi } from '@/api/admin';
import { chartColor } from '@/lib/chartTheme';

/** 每用户决策数直方图：窗口内按 user_id 计 learning_records 行数分桶 + P50/P95。
 * 真实数据来自 /api/admin/amas/metrics/decision-histogram。 */
export function DecisionHistogramPanel(props: { days: () => number }) {
  const [d] = createResource(props.days, (days) => adminApi.amasDecisionHistogram(days));

  const option = (): import('echarts').EChartsOption => {
    const b = d()?.buckets ?? [];
    return {
      grid: { left: 44, right: 12, top: 16, bottom: 28 },
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
      xAxis: { type: 'category', data: b.map((x) => x.label), axisLabel: { fontSize: 10 } },
      yAxis: { type: 'value', minInterval: 1, axisLabel: { fontSize: 10 } },
      series: [
        {
          type: 'bar',
          data: b.map((x) => x.count),
          itemStyle: { color: chartColor('accent'), borderRadius: [3, 3, 0, 0] },
          barWidth: '56%',
        },
      ],
    };
  };

  return (
    <Card variant="elevated" class="amas-panel">
      <div class="amas-panel-head">
        <h3>每用户决策数直方图</h3>
        <span class="amas-sub">{props.days()}d · {(d()?.totalUsers ?? 0).toLocaleString()} 用户</span>
      </div>
      <Show when={!d.loading} fallback={<div class="min-h-[240px] flex items-center justify-center"><Spinner /></div>}>
        <Show when={(d()?.totalUsers ?? 0) > 0} fallback={<Empty title="暂无答题记录" description="learning_records 当前窗口无数据" />}>
          <EChart option={option} height="220px" />
          <div class="flex justify-between text-sm mt-3">
            <span class="text-content-secondary">P50 学习强度</span>
            <strong class="font-mono tabular-nums">{d()?.p50 ?? 0} 题 / 窗口</strong>
          </div>
          <div class="flex justify-between text-sm mt-2">
            <span class="text-content-secondary">P95 学习强度</span>
            <strong class="font-mono tabular-nums">{d()?.p95 ?? 0} 题 / 窗口</strong>
          </div>
        </Show>
      </Show>
    </Card>
  );
}
