import { createResource, Show } from 'solid-js';
import { Spinner } from '@/components/ui/Spinner';
import { TechTip } from '@/components/ui/TechTip';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import { compact } from './util';
import {
  activeProbesHealth,
  eventsHealth,
  queueHealth,
  errorRateHealth,
  overallHealth,
  HEALTH_LABEL,
  type KpiHealth,
} from './readable';

/** 顶部整体健康 banner + 4 张 KPI 卡（活跃探针 / 上报事件 / 队列积压 / 采集错误率）。
 *  每卡：业务解读 + 健康状态点（正常/注意/异常）+ 技术口径悬浮（ⓘ），让非技术客服直接看懂。 */
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

  const winLabel = () => (props.days() === 1 ? '24h' : `${props.days()}d`);

  return (
    <div class="animate-fade-in-up">
      {/* 整体健康一句话总结 */}
      <Show when={data()}>
        {(d) => {
          const h = overallHealth(d(), winLabel());
          return (
            <div class={`pb-health is-${h.level}`}>
              <span class="pb-health-dot" />
              <span class="pb-health-text">
                <b>系统健康度{HEALTH_LABEL[h.level]}</b> · {h.sentence}
              </span>
            </div>
          );
        }}
      </Show>

      <div class="pb-kpi-grid">
        <Show
          when={data()}
          fallback={
            <div class="pb-kpi" style={{ 'grid-column': '1 / -1', 'min-height': '120px', display: 'grid', 'place-items': 'center' }}>
              <Spinner />
            </div>
          }
        >
          {(d) => {
            const ap = () => activeProbesHealth(d().activeProbes.value, d().activeProbes.total ?? 4);
            const ev = () => eventsHealth(d().events24h.value, d().events24h.deltaPct);
            const qb = () => queueHealth(d().queueBacklog.value);
            const er = () => errorRateHealth(d().collectErrorRate.value);
            return (
              <>
                {/* 活跃探针 */}
                <div class="pb-kpi is-primary">
                  <div class="pulse-ring" />
                  <div class="lbl">
                    <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="5" /></svg>
                    活跃探针
                    <StatusPill h={ap()} />
                    <TechTip label="活跃探针口径" width="290px">
                      <b>技术口径</b>
                      <span class="tip-mono">{d().activeProbes.note ?? '有数据的派生探针数 / 总定义数'}</span>
                      <span class="tip-note">0 数据不一定是故障，可能该端没有这类上报或采样已关。</span>
                    </TechTip>
                  </div>
                  <div class="v">
                    {d().activeProbes.value}
                    <Show when={d().activeProbes.total != null}>
                      <span class="unit">/ {d().activeProbes.total}</span>
                    </Show>
                  </div>
                  <div class="bar-mini"><span style={{ width: `${activePct()}%` }} /></div>
                  <div class="pb-kpi-meaning">{ap().meaning}</div>
                  <Show when={ap().hint}><div class="pb-kpi-hint">{ap().hint}</div></Show>
                </div>

                {/* 上报事件 */}
                <div class="pb-kpi">
                  <div class="lbl">
                    <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12" /></svg>
                    {winLabel()} 上报事件
                    <StatusPill h={ev()} />
                    <TechTip label="上报事件口径" width="290px">
                      <b>技术口径</b>
                      <span class="tip-mono">{d().events24h.note ?? 'learning_records + telemetry_events'}</span>
                    </TechTip>
                  </div>
                  <div class="v">{compact(d().events24h.value)}<span class="unit">条</span></div>
                  <div class="pb-kpi-meaning">{ev().meaning}</div>
                  <Show
                    when={d().events24h.deltaPct != null}
                    fallback={<div class="pb-kpi-hint">无前一时段可比基准</div>}
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
                    <StatusPill h={qb()} />
                    <TechTip label="队列积压口径" width="290px" align="right">
                      <b>技术口径</b>
                      <span class="tip-mono">{d().queueBacklog.note ?? 'probe_executions 未完成(completed_at IS NULL)'}</span>
                    </TechTip>
                  </div>
                  <div class="v">{compact(d().queueBacklog.value)}<span class="unit">条</span></div>
                  <div class="pb-kpi-meaning">{qb().meaning}</div>
                  <Show when={qb().hint}><div class="pb-kpi-hint">{qb().hint}</div></Show>
                </div>

                {/* 采集错误率 */}
                <div class="pb-kpi">
                  <div class="lbl">
                    <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10" /><line x1="12" y1="8" x2="12" y2="12" /><line x1="12" y1="16" x2="12.01" y2="16" /></svg>
                    采集错误率
                    <StatusPill h={er()} />
                    <TechTip label="采集错误率口径" width="290px" align="right">
                      <b>技术口径</b>
                      <span class="tip-mono">{d().collectErrorRate.note ?? '有错误的事件数 / 事件总数'}</span>
                    </TechTip>
                  </div>
                  <div class="v">{(d().collectErrorRate.value * 100).toFixed(2)}<span class="unit">%</span></div>
                  <div class="pb-kpi-meaning">{er().meaning}</div>
                  <Show when={er().hint}><div class="pb-kpi-hint">{er().hint}</div></Show>
                </div>
              </>
            );
          }}
        </Show>
      </div>
    </div>
  );
}

/** 健康状态小药丸：彩点 + 正常/注意/异常。 */
function StatusPill(props: { h: KpiHealth }) {
  return (
    <span class={`pb-kpi-status is-${props.h.level}`}>
      <i />
      {HEALTH_LABEL[props.h.level]}
    </span>
  );
}
