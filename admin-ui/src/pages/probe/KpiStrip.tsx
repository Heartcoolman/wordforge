import { createResource, Show } from 'solid-js';
import { Spinner } from '@/components/ui/Spinner';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import { compact } from './util';

/** 4 张真实 KPI 卡：活跃探针 / 24h 事件 / 队列积压 / 采集错误率。
 *  首卡 is-primary 含脉动环 + 迷你进度条；events24h 卡含 delta（无基准时省略）。 */
export function KpiStrip(props: { days: () => number }) {
  const [data] = createResource(props.days, (d) => probeTelemetryApi.overview(d));

  const activePct = () => {
    const a = data()?.activeProbes;
    if (!a || !a.total) return 0;
    return Math.min(100, Math.round((a.value / a.total) * 100));
  };

  const deltaCls = () => {
    const d = data()?.events24h.deltaPct;
    if (d == null) return '';
    return d > 0 ? 'up' : d < 0 ? 'down' : '';
  };

  // 窗口标签：1 天→24h，其余→Nd（与后端 note 一致）。
  const winLabel = () => (props.days() === 1 ? '24h' : `${props.days()}d`);

  return (
    <div class="pb-kpi-grid animate-fade-in-up">
      <Show
        when={data()}
        fallback={
          <div class="pb-kpi" style={{ 'grid-column': '1 / -1', 'min-height': '120px', display: 'grid', 'place-items': 'center' }}>
            <Spinner />
          </div>
        }
      >
        {(d) => (
          <>
            {/* 活跃探针 */}
            <div class="pb-kpi is-primary">
              <div class="pulse-ring" />
              <div class="lbl">
                <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="5" /></svg>
                活跃探针
              </div>
              <div class="v">
                {d().activeProbes.value}
                <Show when={d().activeProbes.total != null}>
                  <span class="unit">/ {d().activeProbes.total}</span>
                </Show>
              </div>
              <div class="bar-mini"><span style={{ width: `${activePct()}%` }} /></div>
              <div class="delta up" style={{ 'margin-top': '8px' }}>
                {activePct()}% 有 24h 数据{d().activeProbes.note ? ` · ${d().activeProbes.note}` : ''}
              </div>
            </div>

            {/* 24h 上报事件 */}
            <div class="pb-kpi">
              <div class="lbl">
                <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12" /></svg>
                {winLabel()} 上报事件
              </div>
              <div class="v">{compact(d().events24h.value)}<span class="unit">条</span></div>
              <Show
                when={d().events24h.deltaPct != null}
                fallback={<div class="delta">无前一窗口可比基准</div>}
              >
                <div class={`delta ${deltaCls()}`}>
                  {(d().events24h.deltaPct ?? 0) >= 0 ? '▲' : '▼'} {Math.abs((d().events24h.deltaPct ?? 0) * 100).toFixed(1)}% vs 前 {winLabel()}
                </div>
              </Show>
            </div>

            {/* 队列积压 */}
            <div class="pb-kpi">
              <div class="lbl">
                <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="16" rx="2" /><line x1="3" y1="10" x2="21" y2="10" /></svg>
                队列积压
              </div>
              <div class="v">{compact(d().queueBacklog.value)}<span class="unit">条</span></div>
              <div class="delta">{d().queueBacklog.note ?? 'probe_executions 未完成'}</div>
            </div>

            {/* 采集错误率 */}
            <div class="pb-kpi">
              <div class="lbl">
                <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line x1="12" y1="16" x2="12.01" y2="16" /></svg>
                采集错误率
              </div>
              <div class="v">{(d().collectErrorRate.value * 100).toFixed(2)}<span class="unit">%</span></div>
              <div class="delta">{d().collectErrorRate.note ?? 'SUM(error_count) / 24h 事件'}</div>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}
