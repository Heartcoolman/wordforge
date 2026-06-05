import { createResource, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { adminApi, type AmasStageStat } from '@/api/admin';
import { chartColor } from '@/lib/chartTheme';

const find = (stages: AmasStageStat[] | undefined, s: string) =>
  (stages ?? []).find((x) => x.stage === s);

/** 阶段切换比例：稳定 / 冷启动 / 过渡 三态占比 + 7 天趋势线。
 * 真实数据来自 /api/admin/amas/metrics/stage-distribution（按 total_event_count 分类）。 */
export function StageSwitchPanel() {
  const [d] = createResource(() => adminApi.amasStageDistribution());

  const trendOption = (): import('echarts').EChartsOption => {
    const t = d()?.trend ?? [];
    const pct = (v: number) => Number((v * 100).toFixed(1));
    return {
      grid: { left: 36, right: 12, top: 16, bottom: 24 },
      tooltip: { trigger: 'axis', valueFormatter: (v) => `${v}%` },
      legend: { show: false },
      xAxis: { type: 'category', data: t.map((p) => p.date.slice(5)), axisLabel: { fontSize: 10 } },
      yAxis: { type: 'value', axisLabel: { formatter: '{value}%', fontSize: 10 } },
      series: [
        { name: '稳定', type: 'line', smooth: true, showSymbol: false, data: t.map((p) => pct(p.stable)), itemStyle: { color: chartColor('success') }, lineStyle: { width: 2 } },
        { name: '冷启动', type: 'line', smooth: true, showSymbol: false, data: t.map((p) => pct(p.cold)), itemStyle: { color: chartColor('accent') }, lineStyle: { width: 2 } },
        { name: '过渡', type: 'line', smooth: true, showSymbol: false, data: t.map((p) => pct(p.transition)), itemStyle: { color: chartColor('warning') }, lineStyle: { width: 2 } },
      ],
    };
  };

  return (
    <Card variant="elevated" class="amas-panel">
      <div class="amas-panel-head">
        <h3>阶段切换比例</h3>
        <span class="amas-sub">cold-start vs stable</span>
      </div>
      <Show when={!d.loading} fallback={<div class="min-h-[200px] flex items-center justify-center"><Spinner /></div>}>
        <Show when={(d()?.totalUsers ?? 0) > 0} fallback={<Empty title="暂无用户状态" description="engine_user_states 当前为空" />}>
          {(() => {
            const stable = find(d()?.stages, 'stable');
            const cold = find(d()?.stages, 'cold');
            const tran = find(d()?.stages, 'transition');
            const pc = (v?: number) => `${((v ?? 0) * 100).toFixed(0)}%`;
            return (
              <>
                <div class="stage-split">
                  <div>
                    <div class="stage-label">稳定阶段 (≥ 200 次答题)</div>
                    <div class="stage-big" style={{ color: 'var(--success-strong)' }}>{pc(stable?.pct)}</div>
                    <div class="stage-meta">{(stable?.users ?? 0).toLocaleString()} 用户</div>
                  </div>
                  <div>
                    <div class="stage-label">冷启动 (&lt; 15 次)</div>
                    <div class="stage-big" style={{ color: 'var(--accent)' }}>{pc(cold?.pct)}</div>
                    <div class="stage-meta">{(cold?.users ?? 0).toLocaleString()} 用户</div>
                  </div>
                  <div>
                    <div class="stage-label">过渡期</div>
                    <div class="stage-big" style={{ color: 'var(--warning-strong)' }}>{pc(tran?.pct)}</div>
                    <div class="stage-meta">{(tran?.users ?? 0).toLocaleString()} 用户</div>
                  </div>
                </div>
                <div class="amas-divider" />
                <div class="amas-sub" style={{ 'text-transform': 'none' }}>阶段切换趋势（按日快照，随 worker 累积）</div>
                <div style={{ 'margin-top': '6px' }}>
                  <EChart option={trendOption} height="150px" />
                </div>
              </>
            );
          })()}
        </Show>
      </Show>
    </Card>
  );
}
