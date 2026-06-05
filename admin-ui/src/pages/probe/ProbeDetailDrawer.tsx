import { createResource, createMemo, For, Show } from 'solid-js';
import { Drawer } from '@/components/ui/Drawer';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type { ProbeRow } from '@/types/probeTelemetry';
import { fmtEps, ago, ratePct } from './util';

/** 探针行「…」详情抽屉。全部接真实端点:
 *  - 有 samplingEventType(click→periodic)的探针拉 /schema 真实样本 + /audit(按 event_type 客户端过滤);
 *  - 核心数据派生探针(learn/perf,无 samplingEventType)不经采样管道,诚实标注无独立 schema。 */
export function ProbeDetailDrawer(props: { probe: ProbeRow; open: boolean; onClose: () => void }) {
  const et = () => props.probe.samplingEventType;

  const [schema] = createResource(
    () => (props.open && et() ? et()! : null),
    (e) => probeTelemetryApi.schema(e),
  );
  const [auditAll] = createResource(
    () => (props.open && et() ? et()! : null),
    () => probeTelemetryApi.audit(50),
  );
  const auditRows = createMemo(() => {
    const e = et();
    const rows = auditAll()?.rows ?? [];
    return e ? rows.filter((r) => r.eventType === e) : [];
  });

  const Row = (p: { k: string; v: string; mono?: boolean }) => (
    <div class="flex items-baseline justify-between gap-4 py-1.5 border-b border-border-hairline last:border-0">
      <dt class="text-content-tertiary shrink-0">{p.k}</dt>
      <dd class={`text-content text-right ${p.mono ? 'font-mono tabular-nums' : ''}`}>{p.v}</dd>
    </div>
  );

  return (
    <Drawer open={props.open} onClose={props.onClose} title={`探针详情 · ${props.probe.label}`} width={440}>
      <div class="text-[13px]">
        <dl>
          <Row k="探针 key" v={props.probe.key} mono />
          <Row k="数据源" v={props.probe.desc} />
          <Row k="24h 事件" v={props.probe.count24h.toLocaleString()} mono />
          <Row k="EPS" v={fmtEps(props.probe.eps)} mono />
          <Row k="最近一次" v={ago(props.probe.lastTs)} />
          <Row k="采样率" v={`${ratePct(props.probe.sampleRate)}%`} mono />
          <Row
            k="状态"
            v={props.probe.locked ? '锁定 · 核心数据强制 100%' : props.probe.enabled ? '启用' : '已停用'}
          />
          <Show when={et()}>
            <Row k="采样 event_type" v={et()!} mono />
          </Show>
        </dl>

        <Show
          when={et()}
          fallback={
            <p class="mt-5 rounded-lg border border-border-hairline bg-surface-secondary px-3 py-2.5 text-content-secondary leading-relaxed">
              该探针是核心数据（{props.probe.desc}）派生的观测指标，不经 telemetry 采样管道，因此无独立 payload schema，也不可调采样率。
            </p>
          }
        >
          <h4 class="mt-5 mb-2 text-[13px] font-semibold text-content">样本 Schema · {et()}</h4>
          <Show when={!schema.loading} fallback={<div class="py-4 grid place-items-center"><Spinner /></div>}>
            <Show
              when={schema()?.payload != null}
              fallback={<Empty title="暂无样本" description={`${et()} 暂无 telemetry_events 落库样本`} />}
            >
              <pre class="pb-schema">{JSON.stringify(schema()!.payload, null, 2)}</pre>
            </Show>
          </Show>

          <h4 class="mt-5 mb-2 text-[13px] font-semibold text-content">采样变更审计 · {et()}</h4>
          <Show when={!auditAll.loading} fallback={<div class="py-4 grid place-items-center"><Spinner /></div>}>
            <Show
              when={auditRows().length > 0}
              fallback={<Empty title="暂无变更" description="该 event_type 采样配置未改动过" />}
            >
              <ul class="space-y-1.5">
                <For each={auditRows()}>
                  {(r) => (
                    <li class="flex items-baseline gap-3 text-[12px] py-1.5 border-b border-border-hairline last:border-0">
                      <span class="font-mono text-content-tertiary shrink-0">{ago(r.ts)}</span>
                      <span class="text-content-secondary">
                        <b class="text-content uppercase">{r.action}</b>{' '}
                        <span class="font-mono">{r.oldValue ?? '—'}</span> →{' '}
                        <b class="font-mono text-accent">{r.newValue ?? '—'}</b>
                      </span>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </Show>
        </Show>
      </div>
    </Drawer>
  );
}
