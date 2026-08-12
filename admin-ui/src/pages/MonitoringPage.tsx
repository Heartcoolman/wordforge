import { createSignal, createMemo, createEffect, onMount, onCleanup, For, Show } from 'solid-js';
import {
  Btn, Badge, Panel, StatCard, Progress, Empty, Loading, Skel, Modal, Confirm, Switch,
  Seg, LineChart, PageHead, Icon, sx, toast, fmtNum, fmtBytes, fmtDur, fmtAgo,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import type {
  RequestMetrics, LogLine, AlertEvent, SystemHealth, UpdateCheck, DeadLetterEntry,
  WorkerStatusRow, DatabaseInfo,
} from '@/types/admin';
import type { AdvisorCostStats } from '@/api/admin';
import type { AmasMetrics } from '@/types/amas';

type WindowKey = '15m' | '1h' | '6h' | '24h' | '7d';
const WINDOWS: WindowKey[] = ['15m', '1h', '6h', '24h', '7d'];
const FAST_MS = 5_000;   // 请求指标 / 日志 / 告警
const SLOW_MS = 30_000;  // 健康 / worker / 顾问成本 / 更新检查 / DB / 死信

const LOG_COLOR: Record<string, string> = {
  ERROR: 'var(--error)', WARN: 'var(--warning)', INFO: 'var(--text-2)', DEBUG: 'var(--text-3)', TRACE: 'var(--text-3)',
};
const SEV_COLOR: Record<string, string> = {
  error: 'var(--error)', warning: 'var(--warning)', info: 'var(--info)', resolved: 'var(--success)',
};
const WK_COLOR = { up: 'var(--success)', warn: 'var(--warning)', down: 'var(--error)' } as const;

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

/** HH:MM 标签（unix 秒） */
function fmtHHMM(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
}
/** HH:MM:SS 时钟（unix 毫秒） */
function fmtClockMs(ms: number): string {
  return new Date(ms).toLocaleTimeString('zh-CN', { hour12: false });
}
/** worker 心跳 outcome → 色调键 */
function workerTone(w: WorkerStatusRow): 'up' | 'warn' | 'down' {
  const o = (w.lastOutcome ?? '').toLowerCase();
  if (w.lastError || o === 'error' || o === 'failed') return 'down';
  if (['success', 'ok', 'completed', 'skipped', 'idle'].includes(o)) return 'up';
  return 'warn';
}

/** 服务状态行 */
function StatusRow(props: { label: string; ok: boolean; value: string }) {
  return (
    <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 13 })}>
      <span style={sx({ display: 'inline-flex', gap: 7, alignItems: 'center' })}>
        <span class={props.ok ? '' : 'live-dot'} style={sx({ width: 8, height: 8, borderRadius: 50, background: props.ok ? 'var(--success)' : 'var(--error)' })} />
        {props.label}
      </span>
      <span class="mono" style={sx({ fontSize: 12, color: props.ok ? 'var(--text-2)' : 'var(--error)' })}>{props.value}</span>
    </div>
  );
}

/** 资源占用条 */
function ResBar(props: { label: string; value: string; pct: number; tone?: 'accent' | 'success' | 'warning' | 'error' | 'info' }) {
  return (
    <div>
      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 })}>
        <span class="muted" style={sx({ fontSize: 12 })}>{props.label}</span>
        <strong class="mono" style={sx({ fontSize: 12 })}>{props.value}</strong>
      </div>
      <Progress value={clamp(props.pct, 0, 100)} tone={props.tone ?? 'accent'} height={6} />
    </div>
  );
}

export default function MonitoringPage() {
  const [requests, setRequests] = createSignal<RequestMetrics | null>(null);
  const [logs, setLogs] = createSignal<LogLine[]>([]);
  const [events, setEvents] = createSignal<AlertEvent[]>([]);
  const [health, setHealth] = createSignal<SystemHealth | null>(null);
  const [database, setDatabase] = createSignal<DatabaseInfo | null>(null);
  const [workers, setWorkers] = createSignal<WorkerStatusRow[]>([]);
  const [advisorCost, setAdvisorCost] = createSignal<AdvisorCostStats | null>(null);
  const [updateCheck, setUpdateCheck] = createSignal<UpdateCheck | null>(null);
  const [amasMetrics, setAmasMetrics] = createSignal<AmasMetrics | null>(null);

  const [win, setWin] = createSignal<WindowKey>('1h');
  const [logLevel, setLogLevel] = createSignal<'all' | 'INFO' | 'WARN' | 'ERROR'>('all');
  const [autoRefresh, setAutoRefresh] = createSignal(true);
  const [loading, setLoading] = createSignal(true);
  const [clock, setClock] = createSignal(new Date().toLocaleTimeString('zh-CN', { hour12: false }));

  // 死信运维
  const [dlOpen, setDlOpen] = createSignal(false);
  const [dlRows, setDlRows] = createSignal<DeadLetterEntry[]>([]);
  const [dlLoading, setDlLoading] = createSignal(false);
  const [dlPending, setDlPending] = createSignal<{ action: 'requeue' | 'purge'; id: number } | null>(null);
  const [dlBusy, setDlBusy] = createSignal(false);

  let logStreamRef: HTMLDivElement | undefined;

  // ── fetchers ──
  async function fetchRequests() {
    try { setRequests(await adminApi.monitoringRequests(win())); } catch { /* 保留上次 */ }
  }
  async function fetchLogs() {
    try {
      const lvl = logLevel() === 'all' ? undefined : logLevel();
      setLogs((await adminApi.monitoringLogs(200, lvl)).logs ?? []);
    } catch { /* 保留上次 */ }
  }
  async function fetchEvents() {
    try { setEvents((await adminApi.monitoringEvents(6)).events ?? []); } catch { /* 保留上次 */ }
  }
  async function fetchSlow() {
    const [h, w, c, u, m, db] = await Promise.allSettled([
      adminApi.getHealth(),
      adminApi.monitoringWorkers(),
      adminApi.amasAdvisorCost(),
      adminApi.checkUpdate(),
      amasApi.getMetrics(),
      adminApi.getDatabase(),
    ]);
    if (h.status === 'fulfilled') setHealth(h.value);
    if (w.status === 'fulfilled') setWorkers(w.value.workers ?? []);
    if (c.status === 'fulfilled') setAdvisorCost(c.value);
    if (u.status === 'fulfilled') setUpdateCheck(u.value);
    if (m.status === 'fulfilled') setAmasMetrics(m.value);
    if (db.status === 'fulfilled') setDatabase(db.value);
  }

  async function loadDeadLetter() {
    setDlLoading(true);
    try { setDlRows((await adminApi.monitoringDeadLetter(100)).entries ?? []); } catch { /* 保留 */ } finally { setDlLoading(false); }
  }
  function openDeadLetter() { setDlOpen(true); void loadDeadLetter(); }
  async function confirmDlAction() {
    const p = dlPending();
    if (!p) return;
    setDlBusy(true);
    try {
      if (p.action === 'requeue') { await adminApi.monitoringDeadLetterRequeue(p.id); toast.success('已重投', `事件 #${p.id} 已写回 outbox`); }
      else { await adminApi.monitoringDeadLetterPurge(p.id); toast.info('已丢弃', `事件 #${p.id} 已永久删除`); }
      setDlPending(null);
      await loadDeadLetter();
      try { setHealth(await adminApi.getHealth()); } catch { /* 角标刷新失败不阻断 */ }
    } catch (e) {
      toast.error('操作失败', e instanceof Error ? e.message : String(e));
    } finally {
      setDlBusy(false);
    }
  }

  // 切换窗口 / 日志级别立即重取
  createEffect(() => { win(); void fetchRequests(); });
  createEffect(() => { logLevel(); void fetchLogs(); });
  // 日志更新后滚到底部
  createEffect(() => { logs(); if (logStreamRef) logStreamRef.scrollTop = logStreamRef.scrollHeight; });

  async function refreshAll() {
    await Promise.allSettled([fetchSlow(), fetchRequests(), fetchLogs(), fetchEvents()]);
  }

  // 定时器在组件体内同步创建并同步注册 onCleanup：SolidJS 的 onCleanup 只绑定到当前
  // 同步执行的 reactive Owner，一旦放到 async onMount 的 await 之后，Owner 上下文已失效，
  // onCleanup 变为无 owner 的空操作，导致离开本页时三个 setInterval 永不清除而泄漏。
  // 因此与 DevicesPage 一致，把 setInterval + onCleanup 放在同步路径，仅 refreshAll 走异步。
  const clockTimer = setInterval(() => setClock(new Date().toLocaleTimeString('zh-CN', { hour12: false })), 1000);
  const fastTimer = setInterval(() => {
    if (!autoRefresh()) return;
    void fetchRequests(); void fetchLogs(); void fetchEvents();
  }, FAST_MS);
  const slowTimer = setInterval(() => { if (autoRefresh()) void fetchSlow(); }, SLOW_MS);
  onCleanup(() => { clearInterval(clockTimer); clearInterval(fastTimer); clearInterval(slowTimer); });

  onMount(async () => {
    await refreshAll();
    setLoading(false);
  });

  // ── 派生 ──
  const res = () => health()?.resources ?? null;
  // 过载/饱和度（后端 health.saturation）——让可用性卡/服务状态反映过载，不被 5xx 口径的可用性掩盖。
  const sat = () => (health() as any)?.saturation as { degraded?: boolean; reasons?: string[]; p99Ms?: number } | undefined;
  const overloaded = () => !!sat()?.degraded;
  const overloadReasons = () => {
    const map: Record<string, string> = { latency_p99: 'P99延迟', inflight_backlog: '请求堆积', db_pool_exhausted: '写池耗尽', sse_near_cap: 'SSE近上限' };
    return (sat()?.reasons ?? []).map((x) => map[x] ?? x).join('·');
  };
  const workerStats = createMemo(() => {
    const ws = workers();
    return {
      total: ws.length,
      ok: ws.filter((w) => workerTone(w) === 'up').length,
      warn: ws.filter((w) => workerTone(w) === 'warn').length,
      down: ws.filter((w) => workerTone(w) === 'down').length,
    };
  });
  const wkErrCount = () => workerStats().down;

  // AMAS 决策平均延迟（tick），由 metrics 真实推导
  const amasTickMs = createMemo(() => {
    const m = amasMetrics();
    if (!m) return null;
    let calls = 0, lat = 0;
    for (const v of Object.values(m)) { calls += v.callCount; lat += v.totalLatencyUs; }
    return calls > 0 ? lat / calls / 1000 : null;
  });

  // 死信角标（来自 health.outbox 或已加载明细）
  const deadLetterCount = () => health()?.outbox?.deadLetter ?? dlRows().length;

  // 请求指标图：QPS（accent）+ P99（error）双线，labels 取 HH:MM
  const chartLabels = createMemo(() => (requests()?.series ?? []).map((p) => fmtHHMM(p.t)));
  const qpsSeries = createMemo(() => (requests()?.series ?? []).map((p) => Number(p.qps.toFixed(2))));
  const p99Series = createMemo(() => (requests()?.series ?? []).map((p) => Math.round(p.p99Ms)));

  return (
    <div>
      <PageHead
        title="系统监控"
        desc="后端可用性、SLO、请求指标、Worker 心跳、实时日志、告警时间线、领域事件死信与数据库占用。指标随进程生命周期累积，重启清零。"
        right={
          <div style={sx({ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' })}>
            <span class="badge badge-default" style={sx({ display: 'inline-flex', alignItems: 'center', gap: 6 })}>
              <span class="live-dot" style={sx({ width: 7, height: 7, borderRadius: 50, background: 'var(--success)' })} />
              <span class="mono" style={sx({ fontSize: 11.5 })}>实时 · {clock()}</span>
            </span>
            <span style={sx({ display: 'inline-flex', alignItems: 'center', gap: 7 })}>
              <span class="muted-3" style={sx({ fontSize: 12 })}>自动刷新 · 5s</span>
              <Switch checked={autoRefresh()} onChange={setAutoRefresh} />
            </span>
            <Btn variant="secondary" icon="refresh" onClick={() => void refreshAll()}>刷新</Btn>
          </div>
        }
      />

      <Show when={!loading()} fallback={<Loading h={320} />}>
        {/* —— SLO / KPI 卡 —— */}
        <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(200px,1fr))', gap: 16, marginBottom: 16 })}>
          <Show when={requests()} fallback={<><Skel h={116} /><Skel h={116} /><Skel h={116} /><Skel h={116} /></>}>
            {(rq) => (
              <>
                <StatCard
                  tone={health()?.status === 'down' ? 'error' : overloaded() ? 'warning' : rq().availabilityPct >= 99.9 ? 'success' : rq().availabilityPct >= 99 ? 'warning' : 'error'}
                  label="后端可用性" icon="shield"
                  value={rq().availabilityPct.toFixed(2)} unit="%"
                  deltaLabel={overloaded() ? `⚠ 过载降级 · ${overloadReasons()}` : `目标 ≥ 99.9% · 窗口 ${fmtDur(rq().effectiveSecs)}`}
                />
                <StatCard
                  tone="accent" label="平均 RPS" icon="zap"
                  value={rq().qpsAvg.toFixed(1)}
                  deltaLabel={`${fmtNum(rq().totalRequests)} 次 · ${win()} 窗口`}
                />
                <StatCard
                  tone={rq().p99Ms <= 400 ? 'info' : rq().p99Ms <= 800 ? 'warning' : 'error'}
                  label="P99 延迟" icon="clock"
                  value={Math.round(rq().p99Ms)} unit="ms"
                  deltaLabel={`P50 ${Math.round(rq().p50Ms)}ms · 目标 ≤ 400ms`}
                />
                <StatCard
                  tone={rq().errorRate * 100 <= 0.5 ? 'success' : rq().errorRate * 100 <= 1 ? 'warning' : 'error'}
                  label="错误率" icon="alert"
                  value={(rq().errorRate * 100).toFixed(2)} unit="%"
                  deltaLabel={`${rq().total5xx} / ${fmtNum(rq().totalRequests)} 次 5xx`}
                />
              </>
            )}
          </Show>
        </div>

        {/* —— 请求指标图 + 服务状态 —— */}
        <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.5fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })}>
          <Panel
            title="请求指标" sub="QPS / P99 延迟"
            right={<Seg options={WINDOWS} value={win()} onChange={(v) => setWin(v as WindowKey)} />}
          >
            <Show when={(requests()?.series.length ?? 0) > 0} fallback={<Empty title="该窗口暂无请求样本" desc="窗口内尚无到达后端的请求记录。" icon="activity" />}>
              <LineChart
                height={250}
                labels={chartLabels()}
                series={[
                  { name: 'QPS', data: qpsSeries(), color: 'var(--accent)' },
                  { name: 'P99', data: p99Series(), color: 'var(--error)' },
                ]}
                area
              />
            </Show>
          </Panel>

          <Panel
            title="服务状态"
            right={deadLetterCount() > 0
              ? <Btn size="sm" variant="warning" onClick={openDeadLetter}>死信 {deadLetterCount()}</Btn>
              : <span class="muted-3 mono" style={sx({ fontSize: 11.5 })}>live</span>}
          >
            <Show when={health()} fallback={<Loading />}>
              {(h) => (
                <div style={sx({ display: 'flex', flexDirection: 'column', gap: 11 })}>
                  <StatusRow label="HTTP 服务" ok={h().status === 'healthy'} value={h().status === 'down' ? '不可用' : h().status === 'degraded' ? (overloaded() ? '降级 · 过载' : '降级') : '运行正常'} />
                  <StatusRow label="数据库" ok={h().storeProbeOk !== false} value={`${fmtBytes(h().dbSizeBytes)} · WAL`} />
                  <StatusRow label="运行时间" ok value={fmtDur(h().uptimeSecs)} />
                  <StatusRow label="AMAS 引擎" ok={h().services?.amas.healthy !== false}
                    value={amasTickMs() != null ? `${amasTickMs()!.toFixed(amasTickMs()! < 10 ? 2 : 0)}ms tick` : (h().version || '—')} />
                  <StatusRow label="Probe 探针" ok={h().services?.sse.healthy !== false}
                    value={h().services?.sse ? `${h().services!.sse.activeDevices} 设备 · ${h().services!.sse.activeConnections} 连接` : '—'} />
                  <StatusRow label="LLM 顾问"
                    ok={advisorCost() ? advisorCost()!.quotaPct < 100 : true}
                    value={advisorCost() ? `¥${advisorCost()!.monthYuan.toFixed(2)} / ${advisorCost()!.monthCapYuan.toFixed(2)}` : '—'} />
                  <StatusRow label="版本检查"
                    ok={!updateCheck()?.hasUpdate}
                    value={updateCheck() ? `${updateCheck()!.latestVersion} ${updateCheck()!.hasUpdate ? '待安装' : '最新'}` : '—'} />
                  <div class="hr" style={sx({ margin: '4px 0' })} />
                  <ResBar label="CPU"
                    value={res()?.cpuPct != null ? `${res()!.cpuPct!.toFixed(1)}%` : '—'}
                    pct={res()?.cpuPct ?? 0}
                    tone={res()?.cpuPct != null && res()!.cpuPct! > 90 ? 'error' : res()?.cpuPct != null && res()!.cpuPct! > 70 ? 'warning' : 'accent'} />
                  <ResBar label="内存 RSS"
                    value={res()?.memoryRssBytes != null ? `${fmtBytes(res()!.memoryRssBytes)}${res()?.memoryTotalBytes ? ` / ${fmtBytes(res()!.memoryTotalBytes)}` : ''}` : '—'}
                    pct={res()?.memoryRssBytes != null && res()?.memoryTotalBytes ? (res()!.memoryRssBytes! / res()!.memoryTotalBytes!) * 100 : 0} />
                  <ResBar label="DB 连接池"
                    value={res()?.pool ? `${res()!.pool!.connections} / ${res()!.pool!.max}` : '—'}
                    pct={res()?.pool ? (res()!.pool!.connections / Math.max(1, res()!.pool!.max)) * 100 : 0}
                    tone="success" />
                </div>
              )}
            </Show>
          </Panel>
        </div>

        {/* —— Worker 心跳 —— */}
        <Panel
          title="Worker 心跳"
          sub={`${workerStats().total} 个后台任务 · ${workerStats().ok} 正常${workerStats().warn ? ` · ${workerStats().warn} 警告` : ''}${wkErrCount() ? ` · ${wkErrCount()} 异常` : ''}`}
          style={{ 'margin-bottom': '16px' }}
        >
          <Show when={workers().length > 0} fallback={<Empty title="暂无 worker 心跳" desc="worker_last_run 表尚无记录。" icon="cpu" />}>
            <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fill,minmax(160px,1fr))', gap: 9 })}>
              <For each={workers()}>
                {(w) => {
                  const tone = workerTone(w);
                  return (
                    <div style={sx({
                      padding: 11, borderRadius: 11, border: '1px solid var(--hairline)',
                      background: tone === 'down' ? 'var(--error-soft)' : 'var(--surface-sunken)',
                    })}>
                      <div style={sx({ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 6 })}>
                        <span class={tone === 'down' ? 'live-dot' : ''} style={sx({ width: 7, height: 7, borderRadius: 50, background: WK_COLOR[tone] })} />
                        <span style={sx({ fontSize: 11.5, fontWeight: 600, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{w.workerName}</span>
                      </div>
                      <div class="muted-3 mono" style={sx({ fontSize: 10.5 })}>
                        {w.lastDurationMs != null ? `${w.lastDurationMs}ms · ` : ''}{w.lastRunAt ? fmtAgo(w.lastRunAt) : '未运行'}
                      </div>
                      <Show when={w.lastError}>
                        <div style={sx({ fontSize: 10.5, color: 'var(--error)', marginTop: 4, lineHeight: 1.4 })}>{w.lastError}</div>
                      </Show>
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>
        </Panel>

        {/* —— 实时日志 + 告警时间线 —— */}
        <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.5fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })}>
          <Panel
            title="实时日志" sub="ring buffer 尾部"
            right={
              <select
                class="select" style={sx({ width: 'auto', padding: '5px 9px', fontSize: 12 })}
                value={logLevel()} onChange={(e) => setLogLevel(e.currentTarget.value as 'all' | 'INFO' | 'WARN' | 'ERROR')}
              >
                <option value="all">全部级别</option>
                <option value="ERROR">ERROR</option>
                <option value="WARN">WARN</option>
                <option value="INFO">INFO</option>
              </select>
            }
          >
            <div
              ref={(el) => (logStreamRef = el)}
              class="mono"
              style={sx({ maxHeight: 320, overflowY: 'auto', fontSize: 11.5, lineHeight: 1.7, background: 'var(--surface-sunken)', borderRadius: 10, padding: 12 })}
            >
              <Show when={logs().length > 0} fallback={<div class="muted-3" style={sx({ padding: 8 })}>暂无日志（受日志级别过滤约束）</div>}>
                <For each={logs()}>
                  {(l) => (
                    <div style={sx({ display: 'flex', gap: 8 })}>
                      <span class="muted-3" style={sx({ flex: 'none' })}>{fmtClockMs(l.tsMs)}</span>
                      <span style={sx({ color: LOG_COLOR[l.level] ?? 'var(--text-2)', fontWeight: 600, width: 44, flex: 'none' })}>{l.level}</span>
                      <span class="muted-3" style={sx({ flex: 'none', width: 110, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{l.target}</span>
                      <span style={sx({ minWidth: 0 })}>{l.message}</span>
                    </div>
                  )}
                </For>
              </Show>
            </div>
          </Panel>

          <Panel title="告警时间线" sub="最近 6h">
            <Show when={events().length > 0} fallback={<Empty title="暂无告警" desc="近 6h 无 worker 失败 / 版本更新 / AMAS 异常。" icon="bell" />}>
              <div style={sx({ display: 'flex', flexDirection: 'column' })}>
                <For each={events()}>
                  {(e) => {
                    const col = SEV_COLOR[e.severity] ?? 'var(--info)';
                    return (
                      <div style={sx({ display: 'flex', gap: 10, padding: '10px 0', borderBottom: '1px solid var(--hairline)' })}>
                        <span style={sx({ width: 8, height: 8, borderRadius: 50, background: col, marginTop: 5, flex: 'none' })} />
                        <div style={sx({ minWidth: 0 })}>
                          <div style={sx({ fontSize: 12.5, fontWeight: 500 })}>{e.title}</div>
                          <Show when={e.desc}><div class="muted" style={sx({ fontSize: 11.5, marginTop: 2 })}>{e.desc}</div></Show>
                          <div class="muted-3 mono" style={sx({ fontSize: 10.5, marginTop: 2 })}>{fmtHHMM(e.tsMs / 1000)}</div>
                        </div>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </Panel>
        </div>

        {/* —— 数据库 —— */}
        <Panel
          title="数据库"
          sub={database()
            ? `${database()!.walEnabled ? 'WAL' : 'DELETE'} · ${fmtBytes(database()!.sizeOnDisk)} · ${database()!.tableCount} 张表${database()!.walSizeBytes != null ? ` · WAL ${fmtBytes(database()!.walSizeBytes)}` : ''}`
            : ''}
        >
          <Show when={database()} fallback={<Loading />}>
            {(db) => (
              <div style={sx({ overflowX: 'auto' })}>
                <div class="muted-3" style={sx({ fontSize: 12, marginBottom: 10 })}>
                  页大小 {fmtBytes(db().pageSize)} · 页数 {fmtNum(db().pageCount)} · 占用 {fmtBytes(db().sizeOnDisk)}
                </div>
                <Show when={db().tables.length > 0} fallback={<Empty title="无表" icon="database" />}>
                  <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fill,minmax(180px,1fr))', gap: 8 })}>
                    <For each={db().tables}>
                      {(t) => (
                        <div style={sx({ display: 'flex', alignItems: 'center', gap: 8, padding: '8px 11px', borderRadius: 9, background: 'var(--surface-sunken)' })}>
                          <Icon name="database" size={14} />
                          <span class="mono" style={sx({ fontSize: 12, fontWeight: 600, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{t}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            )}
          </Show>
        </Panel>
      </Show>

      {/* —— 领域事件死信运维抽屉 —— */}
      <Modal open={dlOpen()} onClose={() => setDlOpen(false)} title="领域事件死信" size="xl">
        <div class="muted" style={sx({ fontSize: 12.5, marginBottom: 14, lineHeight: 1.6 })}>
          records→AMAS 异步消费重试耗尽 / 永久错误（毒丸）进入死信。可人工重投回 outbox（attempts 归零、立即重新消费）或永久丢弃。
          默认异步路径（RECORDS_OUTBOX_ASYNC=true，v1.3.0 起）下此处展示死信；设 false 回退同步老路时死信恒空。
        </div>
        <Show when={!dlLoading()} fallback={<Loading h={160} />}>
          <Show when={dlRows().length > 0} fallback={<Empty title="无死信" desc="所有领域事件均已成功消费。" icon="inbox" />}>
            <div style={sx({ overflowX: 'auto' })}>
              <table class="tbl">
                <thead>
                  <tr><th>ID</th><th>事件</th><th>用户</th><th>错误</th><th>重试</th><th>进死信</th><th style={sx({ textAlign: 'right' })}>操作</th></tr>
                </thead>
                <tbody>
                  <For each={dlRows()}>
                    {(r) => (
                      <tr>
                        <td class="mono">#{r.id}</td>
                        <td class="mono" style={sx({ fontSize: 11.5 })}>{r.eventType}</td>
                        <td class="muted-3" style={sx({ fontSize: 11.5 })}>{r.userId ? r.userId : '—'}</td>
                        <td style={sx({ fontSize: 12, color: 'var(--error)', maxWidth: 280, wordBreak: 'break-all' })}>{r.lastError}</td>
                        <td class="mono">{r.attempts}</td>
                        <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtAgo(r.deadAt)}</td>
                        <td>
                          <div style={sx({ display: 'flex', gap: 6, justifyContent: 'flex-end' })}>
                            <Btn size="xs" variant="secondary" onClick={() => setDlPending({ action: 'requeue', id: r.id })}>重投</Btn>
                            <Btn size="xs" variant="danger" onClick={() => setDlPending({ action: 'purge', id: r.id })}>丢弃</Btn>
                          </div>
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </Show>
        </Show>
      </Modal>

      <Confirm
        open={dlPending() !== null}
        onClose={() => setDlPending(null)}
        onConfirm={() => void confirmDlAction()}
        title={dlPending()?.action === 'requeue' ? '重投死信？' : '丢弃死信？'}
        body={dlPending()?.action === 'requeue'
          ? `事件 #${dlPending()?.id} 将写回 outbox（attempts 归零、立即重新消费）。`
          : `事件 #${dlPending()?.id} 将被永久删除，无法恢复。`}
        danger={dlPending()?.action === 'purge'}
        confirmText={dlPending()?.action === 'requeue' ? '重投' : '丢弃'}
        loading={dlBusy()}
      />
    </div>
  );
}
