import { createResource, Show } from 'solid-js';
import { A } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { adminApi } from '@/api/admin';
import { chartColor } from '@/lib/chartTheme';

/** 疲劳信号时序：逐日平均强度 + 峰值，触发次数(>=阈值)。
 * 真实数据来自 /api/admin/amas/metrics/fatigue-timeseries。 */
export function FatigueTimeseriesPanel(props: { days: () => number }) {
  const [d] = createResource(props.days, (days) => adminApi.amasFatigueTimeseries(days));

  const option = (): import('echarts').EChartsOption => {
    const pts = d()?.points ?? [];
    return {
      grid: { left: 40, right: 16, top: 16, bottom: 28 },
      tooltip: { trigger: 'axis' },
      legend: { top: 0, right: 0, data: ['平均强度', '峰值'], textStyle: { fontSize: 11 } },
      xAxis: { type: 'category', data: pts.map((p) => p.date.slice(5)), axisLabel: { fontSize: 10 } },
      yAxis: { type: 'value', min: 0, max: 1, axisLabel: { fontSize: 10 } },
      series: [
        { name: '平均强度', type: 'line', smooth: true, showSymbol: false, data: pts.map((p) => Number(p.avgFatigue.toFixed(3))), itemStyle: { color: chartColor('accent') }, lineStyle: { width: 2 } },
        { name: '峰值', type: 'line', smooth: true, showSymbol: false, data: pts.map((p) => Number(p.peakFatigue.toFixed(3))), itemStyle: { color: chartColor('warning') }, lineStyle: { width: 2 }, areaStyle: { opacity: 0.06 } },
      ],
    };
  };

  return (
    <Card variant="elevated" class="amas-panel">
      <div class="amas-panel-head">
        <h3>疲劳信号时序</h3>
        <span class="amas-sub">触发阈值 {(d()?.threshold ?? 0).toFixed(2)} · 触发 / 冷却记录</span>
        <div class="amas-aside">
          <span class="flex items-center gap-1.5 text-xs"><span style="width:8px;height:8px;border-radius:2px;background:var(--accent)" /><span class="text-content-tertiary">平均强度</span><strong class="font-mono tabular-nums">{(d()?.avgIntensity ?? 0).toFixed(2)}</strong></span>
          <span class="flex items-center gap-1.5 text-xs"><span style="width:8px;height:8px;border-radius:2px;background:var(--warning)" /><span class="text-content-tertiary">触发次数</span><strong class="font-mono tabular-nums">{(d()?.totalTriggers ?? 0).toLocaleString()}</strong></span>
        </div>
      </div>
      <Show when={!d.loading} fallback={<div class="min-h-[260px] flex items-center justify-center"><Spinner /></div>}>
        <Show when={(d()?.points?.length ?? 0) > 0} fallback={<Empty title="暂无疲劳采样" description="engine_monitoring_events 当前窗口无数据" />}>
          <EChart option={option} height="260px" />
          <div class="flex justify-between text-sm mt-3">
            <span class="text-content-secondary">疲劳峰值随 LLM advisor patch 应用回落可在此观察</span>
            <A href="/admin/amas-advisor" class="text-accent">查看 patch →</A>
          </div>
        </Show>
      </Show>
    </Card>
  );
}
