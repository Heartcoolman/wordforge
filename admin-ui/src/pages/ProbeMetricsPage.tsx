import { createResource, createSignal, createMemo, createEffect, on, onMount, onCleanup, startTransition, For, Show } from 'solid-js';
import {
  Btn, Badge, Card, Panel, Field, Switch, Empty, Skel, Loading, Spinner, Drawer,
  StatCard, Progress, Seg, PageHead, Icon, sx, cx, toast,
} from '@/components/wf';
import { probeTelemetryApi } from '@/api/probeTelemetry';
import type {
  ProbeOverview, ProbesResponse, ProbeGroup, ProbeRow,
  SamplingResponse, SamplingRule, SinksResponse, SinkStatus,
  SchemaSample, AuditResponse, AuditRow, StreamResponse, StreamEvent,
} from '@/types/probeTelemetry';

/* ====================== 本地常量 / 工具 ====================== */

// 后端按天聚合（最小窗口 1 天），不提供亚天级窗口。
const WINDOWS = [
  { value: '1', label: '24h', days: 1 },
  { value: '7', label: '7d', days: 7 },
  { value: '30', label: '30d', days: 30 },
] as const;

const GROUP_TITLE: Record<string, string> = {
  behavior: '用户行为',
  learn: '学习事件',
  perf: '性能与错误',
};
const GROUP_SUB: Record<string, string> = {
  behavior: '点击、滑动等交互，可按比例采样',
  learn: '开始学习、答题等过程，强制全量保留',
  perf: '前端错误、卡顿等问题，错误不可丢',
};

const EVENT_TYPE_LABEL: Record<string, string> = {
  periodic: '周期上报',
  session_start: '会话开始',
  on_demand: '按需上报',
  error_js: '前端错误',
  error: '错误',
};
const eventTypeLabel = (t: string) => EVENT_TYPE_LABEL[t] ?? t;

const METRIC_LABEL: Record<string, string> = {
  actionsPerMin: '每分钟操作',
  avgResponseTimeMs: '平均响应',
  sessionDurationSecs: '会话时长',
  clickCount: '点击数',
  scrollDepthPct: '滚动深度',
  routeChanges: '页面切换',
  visibilityChanges: '前后台切换',
  currentRoute: '当前页面',
  errorCount: '错误数',
};
const METRIC_UNIT: Record<string, string> = {
  avgResponseTimeMs: ' ms',
  sessionDurationSecs: ' 秒',
  scrollDepthPct: '%',
};
const metricLabel = (k: string) => METRIC_LABEL[k] ?? k;
const metricUnit = (k: string) => METRIC_UNIT[k] ?? '';

/** 大数缩写：1234 → 1.2k，1.5M。 */
function compact(n: number): string {
  if (!Number.isFinite(n)) return '0';
  const abs = Math.abs(n);
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(Math.round(n));
}
/** 采样率 0~1 → 百分比整数。 */
const ratePct = (rate: number) => Math.round(rate * 100);

/** 解析 RFC3339 或 SQLite `YYYY-MM-DD HH:MM:SS`（UTC）→ epoch ms。 */
function parseTs(ts: string): number | null {
  let v = Date.parse(ts);
  if (!Number.isNaN(v)) return v;
  v = Date.parse(ts.replace(' ', 'T') + 'Z');
  return Number.isNaN(v) ? null : v;
}
/** HH:MM:SS（本地时区）。 */
function hms(ts: string): string {
  const t = parseTs(ts);
  if (t == null) return ts.slice(11, 19) || ts;
  const d = new Date(t);
  const p = (x: number) => String(x).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
/** 相对时间。 */
function ago(ts: string | null): string {
  if (!ts) return '无数据';
  const t = parseTs(ts);
  if (t == null) return ts;
  const delta = Math.max(0, Math.floor((Date.now() - t) / 1000));
  if (delta < 5) return '刚刚';
  if (delta < 60) return `${delta} 秒前`;
  if (delta < 3600) return `${Math.floor(delta / 60)} 分钟前`;
  if (delta < 86400) return `${Math.floor(delta / 3600)} 小时前`;
  return `${Math.floor(delta / 86400)} 天前`;
}

/* ====================== 主页面 ====================== */

export default function ProbeMetricsPage() {
  const [win, setWin] = createSignal<string>('1');
  const days = createMemo(() => WINDOWS.find((w) => w.value === win())?.days ?? 1);
  const winLabel = createMemo(() => WINDOWS.find((w) => w.value === win())?.label ?? '24h');

  const [overview, ovActions] = createResource(days, (d) => probeTelemetryApi.overview(d));
  const [probes, probesActions] = createResource(days, (d) => probeTelemetryApi.probes(d));
  const [sampling, samplingActions] = createResource(() => probeTelemetryApi.sampling());
  const [sinks] = createResource(() => probeTelemetryApi.sinks());
  const [audit, auditActions] = createResource(() => probeTelemetryApi.audit(20));

  // schema 抽屉
  const [schemaOpen, setSchemaOpen] = createSignal(false);

  // KPI 派生：events/sec、采集错误率、活跃规则、丢弃率、sink 延迟
  const eventsPerSec = createMemo<number | null>(() => {
    const o = overview();
    if (!o) return null;
    return o.events24h.value / (days() * 86400);
  });
  const errorRate = createMemo<number | null>(() => overview()?.collectErrorRate.value ?? null);
  const activeRules = createMemo<number | null>(() => {
    const s = sampling();
    if (!s) return null;
    return s.rules.filter((r) => r.enabled && r.sampleRate > 0).length;
  });
  // 「丢弃率」= 因采样未留存的比例的估算：1 - 全局默认采样率（无更细粒度真实丢弃指标，取全局默认近似）。
  const droppedPct = createMemo<number | null>(() => {
    const s = sampling();
    if (!s) return null;
    return Math.max(0, 1 - s.globalDefault);
  });
  const sinkLagSecs = createMemo<number | null>(() => {
    const list = sinks()?.sinks;
    if (!list || list.length === 0) return null;
    return Math.max(...list.map((x) => x.lagSecs));
  });

  const refreshAll = () => {
    void ovActions.refetch();
    void probesActions.refetch();
    void samplingActions.refetch();
    void auditActions.refetch();
  };

  const onSamplingChanged = () => {
    void samplingActions.refetch();
    void probesActions.refetch();
    void auditActions.refetch();
  };

  return (
    <div class="space-y-4">
      <PageHead
        title="数据探针"
        desc="客户端遥测的采样组、写入 sink 与实时事件流监控。在此调节采样率与启停采样组，所有改动写入审计。"
        right={
          <>
            <Seg
              options={WINDOWS.map((w) => ({ value: w.value, label: w.label }))}
              value={win()}
              onChange={setWin}
            />
            <Btn variant="secondary" icon="refresh" onClick={refreshAll}>刷新</Btn>
          </>
        }
      />

      {/* ---------- KPI strip ---------- */}
      <KpiStrip
        loading={overview.loading && !overview()}
        eventsPerSec={eventsPerSec()}
        errorRate={errorRate()}
        activeRules={activeRules()}
        droppedPct={droppedPct()}
        sinkLagSecs={sinkLagSecs()}
        winLabel={winLabel()}
        eventsTotal={overview()?.events24h.value ?? null}
        deltaPct={overview()?.events24h.deltaPct ?? null}
      />

      {/* ---------- 采样组 + 写入 sink ---------- */}
      <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) minmax(0,1fr)', gap: 16 })}>
        <Panel title="采样和写入管理" sub="采样组启停与采样率" style={{}}>
          <ProbeGroups
            data={probes()}
            loading={probes.loading && !probes()}
            winLabel={winLabel()}
            onChanged={onSamplingChanged}
          />
        </Panel>
        <Panel title="写入 Sink" sub="后端 SQLite 落地状态" style={{}}>
          <SinksList data={sinks()} loading={sinks.loading && !sinks()} />
        </Panel>
      </div>

      {/* ---------- 实时事件流 ---------- */}
      <EventStreamPanel />

      {/* ---------- 采样规则编辑 + 数据样本 ---------- */}
      <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.4fr) minmax(0,1fr)', gap: 16 })}>
        <Panel
          title="采样级联规则"
          sub={sampling() ? `从上到下命中即停 · 未命中走全局默认 ${ratePct(sampling()!.globalDefault)}%` : '加载中'}
          style={{}}
        >
          <SamplingRules
            data={sampling()}
            loading={sampling.loading && !sampling()}
            onChanged={onSamplingChanged}
          />
        </Panel>
        <Panel
          title="数据样本"
          sub="最新一条 periodic 上报"
          right={<Btn size="sm" variant="secondary" icon="search" onClick={() => setSchemaOpen(true)}>查看 Schema</Btn>}
          style={{}}
        >
          <p class="muted-3" style={sx({ fontSize: 12, lineHeight: 1.6, margin: '0 0 4px' })}>
            点击右上「查看 Schema」查看最新一条上报的真实字段结构，便于核对采集口径。
          </p>
        </Panel>
      </div>

      {/* ---------- 审计轨迹 ---------- */}
      <Panel title="最近改动" sub="采样设置改动历史，便于回溯排查" style={{}}>
        <AuditList data={audit()} loading={audit.loading && !audit()} />
      </Panel>

      <SchemaDrawer open={schemaOpen()} onClose={() => setSchemaOpen(false)} />
    </div>
  );
}

/* ====================== KPI strip ====================== */

function KpiStrip(props: {
  loading: boolean;
  eventsPerSec: number | null;
  errorRate: number | null;
  activeRules: number | null;
  droppedPct: number | null;
  sinkLagSecs: number | null;
  winLabel: string;
  eventsTotal: number | null;
  deltaPct: number | null;
}) {
  const eps = () => {
    const v = props.eventsPerSec;
    if (v == null) return '—';
    return v < 1 ? v.toFixed(2) : v < 10 ? v.toFixed(1) : String(Math.round(v));
  };
  const errTone = () => {
    const r = props.errorRate;
    if (r == null) return 'success' as const;
    const pct = r * 100;
    return pct > 2 ? ('error' as const) : pct >= 0.5 ? ('warning' as const) : ('success' as const);
  };
  const lagTone = () => {
    const l = props.sinkLagSecs;
    if (l == null) return 'success' as const;
    return l > 300 ? ('warning' as const) : ('success' as const);
  };
  return (
    <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(170px,1fr))', gap: 14 })}>
      <Show when={!props.loading} fallback={<Skel h={116} style={{ 'grid-column': '1 / -1' }} />}>
        <StatCard
          tone="accent"
          label="事件/秒"
          icon="zap"
          value={eps()}
          deltaLabel={props.eventsTotal != null ? `${props.winLabel} 累计 ${compact(props.eventsTotal)} 条` : undefined}
          delta={props.deltaPct != null ? props.deltaPct * 100 : undefined}
        />
        <StatCard
          tone={errTone()}
          label="采集错误率"
          icon="alert"
          value={props.errorRate != null ? (props.errorRate * 100).toFixed(2) : '—'}
          unit="%"
          deltaLabel="有错误事件 / 事件总数"
        />
        <StatCard
          tone="info"
          label="活跃规则"
          icon="filter"
          value={props.activeRules ?? '—'}
          deltaLabel="启用且采样率 > 0"
        />
        <StatCard
          tone="warning"
          label="丢弃率"
          icon="x"
          value={props.droppedPct != null ? (props.droppedPct * 100).toFixed(1) : '—'}
          unit="%"
          deltaLabel="全局默认未留存比例"
        />
        <StatCard
          tone={lagTone()}
          label="Sink 延迟"
          icon="clock"
          value={props.sinkLagSecs ?? '—'}
          unit="s"
          deltaLabel="最大写入延迟"
        />
      </Show>
    </div>
  );
}

/* ====================== 采样组（左栏） ====================== */

function ProbeGroups(props: { data: ProbesResponse | undefined; loading: boolean; winLabel: string; onChanged: () => void }) {
  return (
    <Show when={!props.loading} fallback={<Loading h={220} />}>
      <Show
        when={(props.data?.groups.length ?? 0) > 0}
        fallback={<Empty title="暂无探针定义" desc="probe-telemetry /probes 返回空" icon="probe" />}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12 })}>
          <For each={props.data!.groups}>
            {(group) => <GroupCard group={group} winLabel={props.winLabel} onChanged={props.onChanged} />}
          </For>
        </div>
      </Show>
    </Show>
  );
}

function GroupCard(props: { group: ProbeGroup; winLabel: string; onChanged: () => void }) {
  const title = () => GROUP_TITLE[props.group.group] ?? props.group.group;
  const sub = () => GROUP_SUB[props.group.group] ?? props.group.group;
  const total = () => props.group.probes.reduce((s, p) => s + p.count24h, 0);
  const online = () => props.group.probes.filter((p) => p.count24h > 0).length;
  // 组采样率：组内最低非锁可调探针代表；全锁 → 100%。
  const groupRate = () => {
    const adj = props.group.probes.filter((p) => !p.locked && p.samplingEventType);
    if (adj.length === 0) return 100;
    return Math.min(...adj.map((p) => ratePct(p.sampleRate)));
  };
  // 组开关：仅在存在可调探针时生效，切换其绑定 event_type 的 enabled。
  const ctrl = () => props.group.probes.find((p) => !p.locked && p.samplingEventType);
  const groupEnabled = () => ctrl()?.enabled ?? true;
  const toggleGroup = (next: boolean) => {
    const et = ctrl()?.samplingEventType;
    if (!et) return;
    probeTelemetryApi
      .updateSamplingRule(et, { enabled: next })
      .then(() => {
        toast.success(`采集已${next ? '启用' : '停用'}`, `${title()} · ${et}`);
        props.onChanged();
      })
      .catch((err: unknown) => toast.error('操作失败', err instanceof Error ? err.message : '请求失败'));
  };

  return (
    <div style={sx({ borderRadius: 12, border: '1px solid var(--hairline)', overflow: 'hidden', background: 'var(--surface-sunken)' })}>
      <div style={sx({ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 12px', borderBottom: '1px solid var(--hairline)' })}>
        <div style={sx({ flex: 1, minWidth: 0 })}>
          <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
            <span style={sx({ fontWeight: 700, fontSize: 13 })}>{title()}</span>
            <Badge variant={ctrl() ? 'info' : 'default'}>{online()}/{props.group.probes.length} 有数据</Badge>
          </div>
          <div class="muted-3" style={sx({ fontSize: 11, marginTop: 2 })}>{sub()}</div>
        </div>
        <div style={sx({ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 2 })}>
          <span class="mono muted-3" style={sx({ fontSize: 11 })}>{props.winLabel} <b style={sx({ color: 'var(--text)' })}>{compact(total())}</b></span>
          <span class="mono muted-3" style={sx({ fontSize: 11 })}>采样 <b style={sx({ color: 'var(--text)' })}>{groupRate()}%</b></span>
        </div>
        <span title={ctrl() ? '启用 / 停用该组采集' : '核心数据强制采集，不可关闭'}>
          <Switch checked={groupEnabled()} disabled={!ctrl()} onChange={toggleGroup} />
        </span>
      </div>
      <div>
        <For each={props.group.probes}>{(probe) => <ProbeRowView probe={probe} />}</For>
      </div>
    </div>
  );
}

function ProbeRowView(props: { probe: ProbeRow }) {
  const p = () => props.probe;
  const off = () => p().count24h === 0;
  const isError = () => p().key === 'error_js' && p().count24h > 0;
  const dotColor = () => (isError() ? 'var(--error)' : off() ? 'var(--text-3)' : 'var(--success)');
  return (
    <div
      style={sx({
        display: 'flex', alignItems: 'center', gap: 10, padding: '9px 12px',
        borderTop: '1px solid var(--hairline)', opacity: off() ? 0.7 : 1,
      })}
    >
      <span style={sx({ width: 7, height: 7, borderRadius: 50, background: dotColor(), flex: 'none' })} />
      <div style={sx({ flex: 1, minWidth: 0 })}>
        <div style={sx({ fontWeight: 600, fontSize: 12.5 })}>{p().label}</div>
        <div class="muted-3" style={sx({ fontSize: 10.5, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{p().desc}</div>
      </div>
      <div style={sx({ width: 92 })}>
        <Progress value={ratePct(p().sampleRate)} tone={off() ? 'error' : 'accent'} height={6} />
      </div>
      <div class="mono muted-3" style={sx({ fontSize: 11, width: 64, textAlign: 'right', flex: 'none' })}>
        <Show when={p().count24h > 0} fallback="—">
          {p().eps < 1 ? p().eps.toFixed(2) : p().eps.toFixed(1)} EPS
        </Show>
      </div>
      <div style={sx({ width: 70, textAlign: 'right', flex: 'none' })}>
        <Show when={p().locked || !p().samplingEventType}>
          <Badge variant="default">锁定</Badge>
        </Show>
      </div>
    </div>
  );
}

/* ====================== 写入 Sink（右栏） ====================== */

function SinksList(props: { data: SinksResponse | undefined; loading: boolean }) {
  return (
    <Show when={!props.loading} fallback={<Loading h={220} />}>
      <Show
        when={(props.data?.sinks.length ?? 0) > 0}
        fallback={<Empty title="暂无 sink" desc="sinks_status 返回空" icon="db" />}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
          <For each={props.data!.sinks}>{(sink) => <SinkCard sink={sink} />}</For>
        </div>
      </Show>
    </Show>
  );
}

function SinkCard(props: { sink: SinkStatus }) {
  const late = () => props.sink.lagSecs > 300;
  return (
    <div style={sx({ padding: 13, borderRadius: 11, border: `1px solid ${late() ? 'var(--warning)' : 'var(--hairline)'}` })}>
      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 })}>
        <div style={sx({ display: 'flex', gap: 8, alignItems: 'center', minWidth: 0 })}>
          <Icon name="db" size={16} />
          <b style={sx({ fontSize: 13, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{props.sink.label}</b>
          <Badge variant="default">{props.sink.kind}</Badge>
        </div>
        <Badge variant={late() ? 'warning' : 'success'} dot>{late() ? '滞后' : '正常'}</Badge>
      </div>
      <div class="muted-3 mono" style={sx({ fontSize: 11, marginTop: 8 })}>
        {props.sink.id} · {compact(props.sink.rowCount)} 行 · 保留 {props.sink.retentionDays == null ? '永久/不限' : `${props.sink.retentionDays}d`}
        {' · '}
        {props.sink.lastWriteTs ? `延迟 ${props.sink.lagSecs}s` : '无写入'}
      </div>
    </div>
  );
}

/* ====================== 实时事件流 ====================== */

const STREAM_LIMIT = 30;
const POLL_MS = 3000;

type StreamCat = 'learn' | 'perf' | 'err' | 'behavior';
function category(type: string): StreamCat {
  const t = type.toLowerCase();
  if (t.includes('error') || t.includes('err')) return 'err';
  if (t.includes('lesson') || t.includes('word') || t.includes('answer') || t.includes('review')) return 'learn';
  if (t.includes('perf') || t.includes('resource') || t.includes('nav')) return 'perf';
  return 'behavior';
}
const CHIPS: Array<{ id: string; label: string }> = [
  { id: 'all', label: '全部' },
  { id: 'behavior', label: '行为' },
  { id: 'learn', label: '学习' },
  { id: 'perf', label: '性能' },
  { id: 'err', label: '错误' },
];
function catColor(c: StreamCat): string {
  return c === 'err' ? 'var(--error)' : c === 'learn' ? 'var(--success)' : c === 'perf' ? 'var(--warning)' : 'var(--text-2)';
}

function EventStreamPanel() {
  const [data, { refetch }] = createResource(() => probeTelemetryApi.stream(STREAM_LIMIT));
  const [filter, setFilter] = createSignal('all');
  const [paused, setPaused] = createSignal(false);
  const [view, setView] = createSignal<StreamEvent[]>([]);
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());

  onMount(() => {
    const timer = setInterval(() => { if (!paused()) void startTransition(() => refetch()); }, POLL_MS);
    onCleanup(() => clearInterval(timer));
  });
  const togglePause = () => {
    const next = !paused();
    setPaused(next);
    if (!next) void refetch();
  };
  const toggleRaw = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });

  const incoming = createMemo<StreamEvent[]>(() => data()?.events ?? []);
  createEffect(on(incoming, (next) => {
    const prev = view();
    const byId = new Map(prev.map((e) => [e.id, e]));
    setView(prev.length ? next.map((e) => byId.get(e.id) ?? e) : next);
  }));

  const rendered = createMemo(() => {
    const f = filter();
    const list = view();
    return f === 'all' ? list : list.filter((e) => category(e.type) === f);
  });
  const typeCount = createMemo(() => new Set(view().map((e) => e.type)).size);
  const firstLoading = createMemo(() => data.loading && view().length === 0);

  return (
    <Panel
      title="实时事件流"
      sub={`${paused() ? '已暂停' : `每 ${POLL_MS / 1000} 秒刷新`} · ${typeCount()} 类型`}
      right={
        <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
          <span class={cx('live-dot')} style={sx({ opacity: paused() ? 0.3 : 1 })} />
          <Btn size="sm" variant={paused() ? 'secondary' : 'ghost'} icon={paused() ? 'play' : 'clock'} onClick={togglePause}>
            {paused() ? '继续' : '暂停'}
          </Btn>
        </div>
      }
      style={{}}
    >
      <div style={sx({ display: 'flex', gap: 7, flexWrap: 'wrap', marginBottom: 12 })}>
        <For each={CHIPS}>
          {(chip) => (
            <button
              type="button"
              class={cx('badge', filter() === chip.id ? 'badge-accent' : 'badge-default')}
              style={sx({ cursor: 'pointer', border: 'none' })}
              onClick={() => setFilter(chip.id)}
            >
              {chip.label}
            </button>
          )}
        </For>
      </div>
      <div class="mono" style={sx({ maxHeight: 380, overflowY: 'auto', fontSize: 11.5, lineHeight: 1.8 })}>
        <Show when={!firstLoading()} fallback={<Loading h={160} />}>
          <Show
            when={rendered().length > 0}
            fallback={<Empty title="暂无遥测事件" desc="当前没有设备上报，或无匹配筛选" icon="inbox" />}
          >
            <For each={rendered()}>
              {(ev) => <EventCard ev={ev} expanded={expanded().has(ev.id)} onToggle={() => toggleRaw(ev.id)} />}
            </For>
          </Show>
        </Show>
      </div>
    </Panel>
  );
}

function EventCard(props: { ev: StreamEvent; expanded: boolean; onToggle: () => void }) {
  const ev = () => props.ev;
  const cat = () => category(ev().type);
  const dev = () => ev().device;
  return (
    <div style={sx({ padding: '7px 0', borderBottom: '1px solid var(--hairline)' })}>
      <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
        <span class="muted-3" style={sx({ flex: 'none' })}>{hms(ev().ts)}</span>
        <span style={sx({ color: catColor(cat()), fontWeight: 600, flex: 'none' })}>{eventTypeLabel(ev().type)}</span>
        <Badge variant="default" style={{ flex: 'none' }}>设备 {ev().deviceId.slice(0, 8)}…</Badge>
        <span style={sx({ flex: 1 })} />
        <button
          type="button"
          class={cx('badge', props.expanded ? 'badge-accent' : 'badge-default')}
          style={sx({ cursor: 'pointer', border: 'none', flex: 'none' })}
          onClick={props.onToggle}
        >
          {props.expanded ? '收起' : '原始'}
        </button>
      </div>

      <Show when={dev()}>
        {(d) => (
          <div class="muted-3" style={sx({ fontSize: 10.5, marginTop: 3, display: 'flex', gap: 6, alignItems: 'center' })}>
            <span>📱 {[d().os, d().model].filter(Boolean).join(' · ') || '未知设备'}</span>
            <Show when={d().language}>{(l) => <span>· {l()}</span>}</Show>
            <Show when={d().online != null}>
              <span style={sx({ color: d().online ? 'var(--success)' : 'var(--text-3)' })}>· {d().online ? '在线' : '离线'}</span>
            </Show>
          </div>
        )}
      </Show>

      <Show when={ev().metrics.length > 0}>
        <div style={sx({ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 4 })}>
          <For each={ev().metrics}>
            {(m) => (
              <span class="muted-3" style={sx({ fontSize: 10.5 })}>
                {metricLabel(m.key)} <b style={sx({ color: 'var(--text-2)' })}>{m.value}{metricUnit(m.key)}</b>
              </span>
            )}
          </For>
        </div>
      </Show>

      <Show when={props.expanded}>
        <pre style={sx({ margin: '6px 0 0', fontSize: 10.5, whiteSpace: 'pre-wrap', wordBreak: 'break-all', color: 'var(--text-2)', background: 'var(--surface-sunken)', borderRadius: 8, padding: 10 })}>
          {prettyJson(ev().payloadRaw)}
        </pre>
      </Show>
    </div>
  );
}

function prettyJson(raw: string): string {
  try { return JSON.stringify(JSON.parse(raw), null, 2); } catch { return raw; }
}

/* ====================== 采样级联规则编辑 ====================== */

function SamplingRules(props: { data: SamplingResponse | undefined; loading: boolean; onChanged: () => void }) {
  const [editing, setEditing] = createSignal<string | null>(null);
  const [draft, setDraft] = createSignal(0);
  const [saving, setSaving] = createSignal(false);

  const beginEdit = (r: SamplingRule) => { setEditing(r.eventType); setDraft(ratePct(r.sampleRate)); };
  const cancel = () => setEditing(null);

  const saveRate = (r: SamplingRule) => {
    const next = draft();
    if (next === ratePct(r.sampleRate)) { cancel(); return; }
    setSaving(true);
    probeTelemetryApi
      .updateSamplingRule(r.eventType, { sampleRate: next / 100 })
      .then(() => { toast.success('采样率已更新', `${r.eventType} → ${next}%`); cancel(); props.onChanged(); })
      .catch((err: unknown) => toast.error('更新失败', err instanceof Error ? err.message : '请求失败'))
      .finally(() => setSaving(false));
  };
  const toggleEnabled = (r: SamplingRule, enabled: boolean) => {
    probeTelemetryApi
      .updateSamplingRule(r.eventType, { enabled })
      .then(() => { toast.success(enabled ? '规则已启用' : '规则已暂停', r.eventType); props.onChanged(); })
      .catch((err: unknown) => toast.error('操作失败', err instanceof Error ? err.message : '请求失败'));
  };

  return (
    <Show when={!props.loading} fallback={<Loading h={160} />}>
      <Show
        when={(props.data?.rules.length ?? 0) > 0}
        fallback={<Empty title="暂无采样规则" desc="probe_sampling_config 表为空" icon="filter" />}
      >
        <p class="muted-3" style={sx({ fontSize: 12, lineHeight: 1.6, margin: '0 0 10px' })}>
          采样 = 按比例留存数据：100% 全部记录、50% 每两条留一条、0% 全部不记。核心数据强制 100% 锁定。
        </p>
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 6 })}>
          <For each={props.data!.rules}>
            {(rule) => {
              const off = () => !rule.enabled || rule.sampleRate === 0;
              const isEditing = () => editing() === rule.eventType;
              return (
                <div
                  style={sx({
                    display: 'flex', alignItems: 'center', gap: 10, padding: '9px 11px',
                    borderRadius: 10, background: 'var(--surface-sunken)', opacity: off() ? 0.72 : 1,
                  })}
                >
                  <div style={sx({ flex: 1, minWidth: 0 })}>
                    <code class="mono" style={sx({ fontSize: 12, fontWeight: 600 })}>{eventTypeLabel(rule.eventType)}</code>
                    <div class="muted-3" style={sx({ fontSize: 10.5, marginTop: 1 })}>
                      {rule.target ? `绑定探针 ${rule.target}` : '—'}
                      {rule.locked ? ' · 核心数据强制' : ''}
                      {!rule.enabled ? ' · 已暂停' : ''}
                    </div>
                  </div>

                  <Show
                    when={isEditing()}
                    fallback={
                      <span class="mono tnum" style={sx({ fontSize: 13, fontWeight: 700, color: off() ? 'var(--error)' : 'var(--text)', width: 48, textAlign: 'right' })}>
                        {ratePct(rule.sampleRate)}%
                      </span>
                    }
                  >
                    <span style={sx({ display: 'inline-flex', alignItems: 'center', gap: 4 })}>
                      <input
                        class="input mono"
                        type="number"
                        min="0"
                        max="100"
                        style={sx({ width: 64, padding: '4px 6px', fontSize: 12 })}
                        value={draft()}
                        disabled={rule.locked}
                        aria-label={`${rule.eventType} 采样率`}
                        onInput={(e) => setDraft(Math.max(0, Math.min(100, Number(e.currentTarget.value) || 0)))}
                      />
                      <span class="muted-3" style={sx({ fontSize: 12 })}>%</span>
                    </span>
                  </Show>

                  <Show
                    when={isEditing()}
                    fallback={
                      <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
                        <span title={rule.locked ? '核心数据强制采集，不可暂停' : rule.enabled ? '点击暂停采集' : '点击恢复采集'}>
                          <Switch checked={rule.enabled} disabled={rule.locked} onChange={(v) => toggleEnabled(rule, v)} />
                        </span>
                        <Show
                          when={!rule.locked}
                          fallback={<Badge variant="default">锁定</Badge>}
                        >
                          <Btn size="xs" variant="ghost" onClick={() => beginEdit(rule)}>编辑</Btn>
                        </Show>
                      </div>
                    }
                  >
                    <div style={sx({ display: 'flex', gap: 6 })}>
                      <Btn size="xs" variant="primary" disabled={saving()} onClick={() => saveRate(rule)}>保存</Btn>
                      <Btn size="xs" variant="ghost" disabled={saving()} onClick={cancel}>取消</Btn>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>
      </Show>
    </Show>
  );
}

/* ====================== Schema 预览抽屉 ====================== */

const SCHEMA_EVENT_TYPE = 'periodic';

function SchemaDrawer(props: { open: boolean; onClose: () => void }) {
  const [data] = createResource(
    () => (props.open ? SCHEMA_EVENT_TYPE : undefined),
    (et) => probeTelemetryApi.schema(et),
  );
  return (
    <Drawer
      open={props.open}
      onClose={props.onClose}
      title={`数据样本 · ${SCHEMA_EVENT_TYPE}`}
      subtitle={data()?.sampledAt ? `采样于 ${data()!.sampledAt!.slice(0, 19).replace('T', ' ')}` : '无样本'}
      width={520}
    >
      <p class="muted-3" style={sx({ fontSize: 12, lineHeight: 1.6, margin: '0 0 12px' })}>
        最新一条 {SCHEMA_EVENT_TYPE} 类型上报的真实字段结构——能看出系统具体记了哪些信息。没有就表示这类事件暂时没人上报。
      </p>
      <Show when={data.state !== 'pending'} fallback={<Loading h={200} />}>
        <Show
          when={data()?.payload != null}
          fallback={<Empty title="暂无样本" desc={`telemetry_events 无 ${SCHEMA_EVENT_TYPE} 类型上报`} icon="inbox" />}
        >
          <pre
            class="mono"
            style={sx({ margin: 0, fontSize: 11.5, lineHeight: 1.65, whiteSpace: 'pre-wrap', wordBreak: 'break-all', color: 'var(--text-2)', background: 'var(--surface-sunken)', borderRadius: 10, padding: 14 })}
          >
            {JSON.stringify(data()!.payload, null, 2)}
          </pre>
        </Show>
      </Show>
    </Drawer>
  );
}

/* ====================== 审计轨迹 ====================== */

const ACTION_VERB: Record<string, string> = {
  add: '新增', mod: '修改', pause: '暂停/恢复', del: '删除',
};
const actionVerb = (a: string) => ACTION_VERB[a] ?? '改动';
const actionVariant = (a: string): 'success' | 'warning' | 'error' | 'info' =>
  a === 'add' ? 'success' : a === 'del' ? 'error' : a === 'pause' ? 'warning' : 'info';

function AuditList(props: { data: AuditResponse | undefined; loading: boolean }) {
  return (
    <Show when={!props.loading} fallback={<Loading h={120} />}>
      <Show
        when={(props.data?.rows.length ?? 0) > 0}
        fallback={<Empty title="暂无改动记录" desc="probe_sampling_audit 表为空" icon="clock" />}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column' })}>
          <For each={props.data!.rows}>{(row) => <AuditRowView row={row} />}</For>
        </div>
      </Show>
    </Show>
  );
}

function AuditRowView(props: { row: AuditRow }) {
  const r = () => props.row;
  const who = () => r().adminId ?? '系统';
  return (
    <div style={sx({ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 0', borderBottom: '1px solid var(--hairline)' })}>
      <span class="muted-3 mono" style={sx({ fontSize: 11.5, flex: 'none', width: 72 })}>{hms(r().ts)}</span>
      <div style={sx({ flex: 1, minWidth: 0, fontSize: 12.5 })}>
        <b>{who()}</b> {actionVerb(r().action)}{' '}
        <code class="mono" style={sx({ fontSize: 11.5 })}>{r().eventType}</code>
        <Show when={r().oldValue && r().newValue}>
          {' '}<span class="muted-3">{r().oldValue}</span>
          {' → '}<span style={sx({ color: 'var(--accent)' })}>{r().newValue}</span>
        </Show>
      </div>
      <Badge variant={actionVariant(r().action)}>{actionVerb(r().action)}</Badge>
    </div>
  );
}
