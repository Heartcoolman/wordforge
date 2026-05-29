import { createResource, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { algorithmColor } from '@/lib/chartTheme';
import { adminApi } from '@/api/admin';

// routing_algo 落库为小写（与 algorithm_metrics_daily 口径一致），此处映射回 algorithmColor 的 PascalCase 键 + 展示名
const DISPLAY: Record<string, string> = {
  heuristic: 'Heuristic',
  ige: 'IGE',
  swd: 'SWD',
  ensemble: 'Ensemble',
  mdm: 'MDM',
  mastery: 'Mastery',
};
const label = (algo: string) => DISPLAY[algo] ?? algo;

/** PR-1:8 算法决策分布甜甜圈 —— 按 routing_algo 聚合 N 天决策占比。
 * 数据源 /api/admin/amas/metrics/algorithm-distribution。 */
export function AlgorithmDonut(props: { days: () => number }) {
  const [dist] = createResource(props.days, (d) => adminApi.amasAlgorithmDistribution(d));
  const total = () => (dist() ?? []).reduce((acc, s) => acc + s.count, 0);

  const option = (): import('echarts').EChartsOption => {
    const data = (dist() ?? []).map((s) => ({
      name: label(s.algorithm),
      value: s.count,
      itemStyle: { color: algorithmColor(label(s.algorithm)) },
    }));
    return {
      tooltip: { trigger: 'item', formatter: '{b}: {c} ({d}%)' },
      legend: { orient: 'vertical', right: 8, top: 'center', itemWidth: 12, textStyle: { fontSize: 12 } },
      series: [
        {
          type: 'pie',
          radius: ['55%', '80%'],
          center: ['36%', '50%'],
          avoidLabelOverlap: true,
          itemStyle: { borderRadius: 4, borderWidth: 2, borderColor: 'transparent' },
          label: { show: false },
          labelLine: { show: false },
          data,
        },
      ],
    };
  };

  return (
    <Card variant="elevated">
      <div class="flex items-baseline justify-between mb-3">
        <h2 class="text-lg font-semibold text-content">算法决策分布</h2>
        <span class="text-xs text-content-tertiary tabular-nums">{total().toLocaleString()} 次决策</span>
      </div>
      <Show
        when={!dist.loading}
        fallback={<div class="min-h-[300px] flex items-center justify-center"><Spinner /></div>}
      >
        <Show
          when={(dist() ?? []).length > 0}
          fallback={
            <Empty
              title="暂无决策路由数据"
              description="engine_monitoring_events.routing_algo 当前为空（PR-0 埋点生效后随决策流量累积）"
            />
          }
        >
          <EChart option={option} height="300px" />
        </Show>
      </Show>
    </Card>
  );
}
