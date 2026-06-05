import { createResource, For } from 'solid-js';
import { StatCard } from '@/components/ui/StatCard';
import { adminApi } from '@/api/admin';

/** PR-1:metrics tab 顶部 4 KPI —— 决策总数 / 命中率 / 平均决策耗时 / 疲劳触发率。
 * 数据源 /api/admin/amas/metrics/kpi，随 days 时间窗口联动刷新。 */
export function KpiCards(props: { days: () => number }) {
  const [kpi] = createResource(props.days, (d) => adminApi.amasMetricsKpi(d));

  const cards = () => {
    const k = kpi();
    return [
      {
        title: `${props.days()} 天决策总数`,
        value: k?.decisionTotal ?? 0,
        color: 'accent' as const,
        icon: 'M8.25 6.75h12M8.25 12h12m-12 5.25h12M3.75 6.75h.008v.008H3.75V6.75zm0 5.25h.008v.008H3.75V12zm0 5.25h.008v.008H3.75v-.008z',
        format: (v: number) => Math.round(v).toLocaleString(),
      },
      {
        title: '命中率',
        value: k?.hitRate ?? 0,
        color: 'success' as const,
        icon: 'M9 12.75L11.25 15 15 9.75M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
        format: (v: number) => `${(v * 100).toFixed(1)}%`,
      },
      {
        title: '平均决策耗时',
        value: k?.avgLatencyMs ?? 0,
        color: 'info' as const,
        icon: 'M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z',
        format: (v: number) => `${v.toFixed(2)} ms`,
      },
      {
        title: '疲劳触发率',
        value: k?.fatigueTriggerRate ?? 0,
        color: 'warning' as const,
        icon: 'M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z',
        format: (v: number) => `${(v * 100).toFixed(1)}%`,
      },
    ];
  };

  return (
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
      <For each={cards()}>
        {(c) => (
          <StatCard
            title={c.title}
            value={c.value}
            color={c.color}
            icon={c.icon}
            format={c.format}
          />
        )}
      </For>
    </div>
  );
}
