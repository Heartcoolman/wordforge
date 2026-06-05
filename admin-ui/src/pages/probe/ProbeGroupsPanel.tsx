import { createResource, createSignal, For, Show } from 'solid-js';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { uiStore } from '@/stores/ui';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type { ProbeGroup, ProbeRow } from '@/types/probeTelemetry';
import { SamplingSlider } from './SamplingSlider';
import { ProbeDetailDrawer } from './ProbeDetailDrawer';
import { GROUP_META, LockIcon, KebabIcon, fmtEps, ago, ratePct } from './util';
import { GROUP_PLAIN, PROBE_PLAIN } from './readable';

/** 左栏：behavior / learn / perf 三组真实探针。每组 head（图标 + 标题 + 24h/采样/EPS 统计），
 *  每行 状态点 + 名称/desc + 采样滑杆 + EPS + lastTs + 锁标记。 */
export function ProbeGroupsPanel(props: { days: () => number }) {
  const [data, { refetch }] = createResource(props.days, (d) => probeTelemetryApi.probes(d));

  return (
    <Show
      when={!data.loading}
      fallback={<div class="min-h-[200px] grid place-items-center"><Spinner /></div>}
    >
      <Show
        when={(data()?.groups.length ?? 0) > 0}
        fallback={<Empty title="暂无探针定义" description="probe-telemetry /probes 返回空" />}
      >
        <For each={data()!.groups}>
          {(group, i) => (
            <GroupCard
              group={group}
              delay={60 + i() * 60}
              onSaved={refetch}
              winLabel={() => (props.days() === 1 ? '24h' : `${props.days()}d`)}
            />
          )}
        </For>
      </Show>
    </Show>
  );
}

function GroupCard(props: { group: ProbeGroup; delay: number; onSaved: () => void; winLabel: () => string }) {
  const meta = () => GROUP_META[props.group.group] ?? { title: props.group.group, sub: props.group.group, cls: '', icon: null };
  const plain = () => GROUP_PLAIN[props.group.group] ?? { sub: meta().sub, meaning: '' };
  const total24h = () => props.group.probes.reduce((s, p) => s + p.count24h, 0);
  const online = () => props.group.probes.filter((p) => p.count24h > 0).length;
  // 组采样率：取组内最低非锁探针的采样率代表；全锁组显示 100%
  const groupRate = () => {
    const adj = props.group.probes.filter((p) => !p.locked && p.samplingEventType);
    if (adj.length === 0) return 100;
    return Math.min(...adj.map((p) => ratePct(p.sampleRate)));
  };
  // 组启用开关：仅在组内存在「可调」探针（非锁 + 绑定真实 telemetry event_type）时真实生效，
  // 切换该 event_type 的 enabled（PATCH /sampling/:et）；否则（learn/perf 核心数据全锁）开关 disabled 恒开。
  const ctrl = () => props.group.probes.find((p) => !p.locked && p.samplingEventType);
  const groupEnabled = () => ctrl()?.enabled ?? true;
  const toggleGroup = (e: Event & { currentTarget: HTMLInputElement }) => {
    const c = ctrl();
    const et = c?.samplingEventType;
    if (!et) return;
    const next = e.currentTarget.checked;
    probeTelemetryApi
      .updateSamplingRule(et, { enabled: next })
      .then(() => {
        uiStore.toast.success(`采集已${next ? '启用' : '停用'}`, `${meta().title} · ${et}`);
        props.onSaved();
      })
      .catch((err: unknown) => {
        e.currentTarget.checked = !next; // 失败回滚视图
        uiStore.toast.error('操作失败', err instanceof Error ? err.message : '请求失败');
      });
  };

  return (
    <div class="pb-group animate-fade-in-up" style={{ 'animation-delay': `${props.delay}ms` }}>
      <div class={`pb-group-head ${meta().cls}`}>
        <div class="ic">{meta().icon}</div>
        <div>
          <div class="title">{meta().title}</div>
          <div class="sub">
            {plain().sub} · {online()}/{props.group.probes.length} 有数据
            <span class="pb-info" tabindex="0" role="img" aria-label="技术来源" title={`技术来源：${meta().sub}`}>ⓘ</span>
          </div>
          <Show when={plain().meaning}><div class="pb-group-meaning">{plain().meaning}</div></Show>
        </div>
        <div class="stats">
          <span>{props.winLabel()} <b>{total24h().toLocaleString()}</b></span>
          <span>采样 <b>{groupRate()}%</b></span>
        </div>
        <label
          class="wf-switch switch"
          title={ctrl() ? '启用 / 停用该组采集' : '核心数据强制采集，不可关闭'}
        >
          <input
            type="checkbox"
            checked={groupEnabled()}
            disabled={!ctrl()}
            onChange={toggleGroup}
            aria-label={`${meta().title} 采集开关`}
          />
          <span class="slider" />
        </label>
      </div>
      <div>
        <For each={props.group.probes}>
          {(probe) => <ProbeRowView probe={probe} onSaved={props.onSaved} />}
        </For>
      </div>
    </div>
  );
}

function ProbeRowView(props: { probe: ProbeRow; onSaved: () => void }) {
  const p = () => props.probe;
  const plain = () => PROBE_PLAIN[p().key] ?? { desc: p().desc, meaning: '' };
  const isError = () => p().key === 'error_js' && p().count24h > 0;
  const isOff = () => p().count24h === 0;
  const rowCls = () => `pb-probe-row${isOff() ? ' is-off' : ''}${isError() ? ' is-error' : ''}`;
  const [detail, setDetail] = createSignal(false);

  return (
    <div class={rowCls()}>
      <span class="state-dot" />
      <div class="name">
        <strong>{p().label}</strong>
        <div class="desc">
          {plain().desc}
          <span
            class="pb-info"
            tabindex="0"
            role="img"
            aria-label="探针来源"
            title={`技术来源：${p().key} · ${p().desc}${plain().meaning ? `\n${plain().meaning}` : ''}`}
          >ⓘ</span>
        </div>
      </div>
      <SamplingSlider probe={p()} onSaved={props.onSaved} />
      <div class="rate-pill">
        <Show when={p().count24h > 0} fallback={<span class="unit">—</span>}>
          {fmtEps(p().eps)}<span class="unit">EPS</span>
        </Show>
      </div>
      <div class="sched">
        <span>最近</span>
        <span class="ago">{ago(p().lastTs)}</span>
      </div>
      <div class="actions">
        <Show when={p().locked || !p().samplingEventType}>
          <span class="lock-badge" title="核心数据强制 100%，采样率锁定"><LockIcon /></span>
        </Show>
        <button type="button" title="探针详情" aria-label={`${p().label} 详情`} onClick={() => setDetail(true)}>
          <KebabIcon />
        </button>
      </div>
      <ProbeDetailDrawer probe={p()} open={detail()} onClose={() => setDetail(false)} />
    </div>
  );
}
