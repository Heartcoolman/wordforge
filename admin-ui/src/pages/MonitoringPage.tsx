import { createSignal, createMemo, createEffect, Show, For, onMount, onCleanup } from 'solid-js';
import type { EChartsOption } from 'echarts';
import { EChart } from '@/components/ui/EChart';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { Modal } from '@/components/ui/Modal';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { adminApi, type AdvisorCostStats } from '@/api/admin';
import { amasApi } from '@/api/amas';
import type { RequestMetrics, LogLine, AlertEvent, SystemHealth, UpdateCheck, DeadLetterEntry } from '@/types/admin';
import type { AmasMetrics } from '@/types/amas';
import { formatBytes } from '@/utils/formatters';
import './monitoring.css';

type WindowKey = '15m' | '1h' | '6h' | '24h' | '7d';
const WINDOWS: WindowKey[] = ['15m', '1h', '6h', '24h', '7d'];
const FAST_MS = 5_000; // 设计图「自动刷新 · 5s」：请求指标 / 日志 / 告警
const SLOW_MS = 30_000; // 健康 / 资源 / worker / 顾问成本 / 更新检查

interface WorkerRow {
  workerName: string;
  lastRunAt: number | null;
  lastDurationMs: number | null;
  lastOutcome: string | null;
  lastError: string | null;
}

// ── 格式化 helpers ──
function fmtHHMM(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
}
function fmtClockMs(ms: number): string {
  return new Date(ms).toLocaleTimeString('zh-CN', { hour12: false });
}
function fmtAge(unixSecs: number | null): string {
  if (unixSecs == null || unixSecs <= 0) return '—';
  const d = Math.max(0, Date.now() / 1000 - unixSecs);
  if (d < 60) return `${Math.floor(d)}s`;
  if (d < 3600) return `${Math.floor(d / 60)}m`;
  if (d < 86400) return `${Math.floor(d / 3600)}h`;
  return `${Math.floor(d / 86400)}d`;
}
function fmtWindow(secs: number): string {
  if (secs < 3600) return `${Math.round(secs / 60)} 分钟`;
  if (secs < 86400) return `${(secs / 3600).toFixed(secs % 3600 === 0 ? 0 : 1)} 小时`;
  return `${(secs / 86400).toFixed(secs % 86400 === 0 ? 0 : 1)} 天`;
}
const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

/** 较低更优指标的 SLO 色调 */
function toneLower(v: number, target: number): string {
  if (v <= target) return 'is-ok';
  if (v <= target * 2) return 'is-warn';
  return 'is-bad';
}
/** 可用性色调（较高更优） */
function toneAvail(v: number): string {
  if (v >= 99.9) return 'is-ok';
  if (v >= 99.0) return 'is-warn';
  return 'is-bad';
}

const POLL_LOG_LIMIT = 200;

export default function MonitoringPage() {
  const [requests, setRequests] = createSignal<RequestMetrics | null>(null);
  const [logs, setLogs] = createSignal<LogLine[]>([]);
  const [events, setEvents] = createSignal<AlertEvent[]>([]);
  const [health, setHealth] = createSignal<SystemHealth | null>(null);
  const [workers, setWorkers] = createSignal<WorkerRow[]>([]);
  const [advisorCost, setAdvisorCost] = createSignal<AdvisorCostStats | null>(null);
  const [updateCheck, setUpdateCheck] = createSignal<UpdateCheck | null>(null);
  const [amasMetrics, setAmasMetrics] = createSignal<AmasMetrics | null>(null);

  const [window, setWindow] = createSignal<WindowKey>('1h');
  const [autoRefresh, setAutoRefresh] = createSignal(true);
  const [logLevel, setLogLevel] = createSignal<'all' | 'INFO' | 'WARN' | 'ERROR'>('all');
  const [logPaused, setLogPaused] = createSignal(false);
  const [loading, setLoading] = createSignal(true);
  const [clock, setClock] = createSignal(new Date().toLocaleTimeString('zh-CN', { hour12: false }));

  // W1-2:outbox 死信运维抽屉
  const [dlOpen, setDlOpen] = createSignal(false);
  const [dlRows, setDlRows] = createSignal<DeadLetterEntry[]>([]);
  const [dlLoading, setDlLoading] = createSignal(false);
  // 待确认的破坏性操作（重投 / 丢弃）
  const [dlPending, setDlPending] = createSignal<{ action: 'requeue' | 'purge'; id: number } | null>(null);
  const [dlBusy, setDlBusy] = createSignal(false);

  async function loadDeadLetter() {
    setDlLoading(true);
    try {
      setDlRows((await adminApi.monitoringDeadLetter(200)).entries);
    } catch { /* keep last */ } finally {
      setDlLoading(false);
    }
  }
  function openDeadLetter() {
    setDlOpen(true);
    void loadDeadLetter();
  }
  async function confirmDlAction() {
    const p = dlPending();
    if (!p) return;
    setDlBusy(true);
    try {
      if (p.action === 'requeue') await adminApi.monitoringDeadLetterRequeue(p.id);
      else await adminApi.monitoringDeadLetterPurge(p.id);
      setDlPending(null);
      await loadDeadLetter();
      try { setHealth(await adminApi.getHealth()); } catch { /* 角标刷新失败不阻断 */ }
    } catch { /* 失败保留弹窗,允许重试 */ } finally {
      setDlBusy(false);
    }
  }

  let logStreamRef: HTMLDivElement | undefined;

  // ── fetchers ──
  async function fetchRequests() {
    try { setRequests(await adminApi.monitoringRequests(window())); } catch { /* keep last */ }
  }
  async function fetchLogs() {
    if (logPaused()) return;
    try {
      const lvl = logLevel() === 'all' ? undefined : logLevel();
      const res = await adminApi.monitoringLogs(POLL_LOG_LIMIT, lvl);
      setLogs(res.logs ?? []);
    } catch { /* keep last */ }
  }
  async function fetchEvents() {
    try { setEvents((await adminApi.monitoringEvents(6)).events ?? []); } catch { /* keep last */ }
  }
  async function fetchSlow() {
    const [h, w, c, u, m] = await Promise.allSettled([
      adminApi.getHealth(),
      adminApi.monitoringWorkers(),
      adminApi.amasAdvisorCost(),
      adminApi.checkUpdate(),
      amasApi.getMetrics(),
    ]);
    if (h.status === 'fulfilled') setHealth(h.value);
    if (w.status === 'fulfilled') {
      setWorkers((w.value.workers ?? []).map((r) => ({
        workerName: r.workerName,
        lastRunAt: r.lastRunAt == null ? null : Number(r.lastRunAt),
        lastDurationMs: r.lastDurationMs == null ? null : Number(r.lastDurationMs),
        lastOutcome: r.lastOutcome,
        lastError: r.lastError,
      })));
    }
    if (c.status === 'fulfilled') setAdvisorCost(c.value);
    if (u.status === 'fulfilled') setUpdateCheck(u.value);
    if (m.status === 'fulfilled') setAmasMetrics(m.value);
  }

  // 切换窗口立即重取请求指标
  createEffect(() => { window(); void fetchRequests(); });
  // 切换日志级别 / 取消暂停立即重取
  createEffect(() => { logLevel(); if (!logPaused()) void fetchLogs(); });

  // 日志更新后滚到底部（终端式 live tail）
  createEffect(() => {
    logs();
    if (logStreamRef) logStreamRef.scrollTop = logStreamRef.scrollHeight;
  });

  onMount(async () => {
    await Promise.allSettled([fetchSlow(), fetchRequests(), fetchLogs(), fetchEvents()]);
    setLoading(false);

    const clockTimer = setInterval(() => setClock(new Date().toLocaleTimeString('zh-CN', { hour12: false })), 1000);
    const fastTimer = setInterval(() => {
      if (!autoRefresh()) return;
      void fetchRequests();
      void fetchLogs();
      void fetchEvents();
    }, FAST_MS);
    const slowTimer = setInterval(() => { if (autoRefresh()) void fetchSlow(); }, SLOW_MS);

    onCleanup(() => { clearInterval(clockTimer); clearInterval(fastTimer); clearInterval(slowTimer); });
  });

  // ── 派生 ──
  const workerByName = (names: string[]): WorkerRow | undefined =>
    workers().find((w) => names.includes(w.workerName));
  const workerTone = (w: WorkerRow): string => {
    const o = (w.lastOutcome ?? '').toLowerCase();
    if (w.lastError) return 'is-down';
    if (['success', 'ok', 'completed', 'skipped', 'idle'].includes(o)) return 'is-up';
    return 'is-warn';
  };
  const workerStats = createMemo(() => {
    const ws = workers();
    const ok = ws.filter((w) => workerTone(w) === 'is-up').length;
    const warn = ws.filter((w) => workerTone(w) === 'is-warn').length;
    const down = ws.filter((w) => workerTone(w) === 'is-down').length;
    return { total: ws.length, ok, warn, down };
  });

  const res = () => health()?.resources ?? null;
  const sse = () => health()?.services?.sse ?? null;

  // AMAS 决策平均延迟(tick)= Σ totalLatencyUs / Σ callCount,由现有 amas metrics 真实推导
  const amasTickMs = createMemo(() => {
    const m = amasMetrics();
    if (!m) return null;
    let calls = 0, lat = 0;
    for (const v of Object.values(m)) { calls += v.callCount; lat += v.totalLatencyUs; }
    return calls > 0 ? lat / calls / 1000 : null;
  });

  // P99 spike:窗口时序里的峰值点,显著高于均值或超目标才标注(真实来自 series)
  const p99Spike = createMemo(() => {
    const rq = requests();
    const s = rq?.series ?? [];
    if (!rq || s.length === 0) return null;
    let max = s[0];
    for (const p of s) if (p.p99Ms > max.p99Ms) max = p;
    return max.p99Ms > Math.max(rq.p99Ms * 1.3, 400) ? max : null;
  });

  // 综合饱和度徽标:CPU 钉死 / 延迟 SLO 大幅突破 / in-flight 堆积 / nginx 上游积压。
  // 解决"5xx=0 但系统被打挂时面板仍显绿"的盲区——这些信号不依赖 5xx。
  const saturation = createMemo(() => {
    const r = res();
    const rq = requests();
    const reasons: string[] = [];
    let level = 0; // 0 健康 / 1 繁忙 / 2 过载降级
    const bump = (n: number, reason: string) => { if (n > level) level = n; reasons.push(reason); };

    const cpu = r?.cpuPct ?? null;
    if (cpu != null) {
      if (cpu >= 90) bump(2, `CPU ${cpu.toFixed(0)}%`);
      else if (cpu >= 70) bump(1, `CPU ${cpu.toFixed(0)}%`);
    }
    const p99 = rq?.p99Ms ?? null;
    if (p99 != null) {
      if (p99 > 1200) bump(2, `P99 ${Math.round(p99)}ms`);
      else if (p99 > 400) bump(1, `P99 ${Math.round(p99)}ms`);
    }
    const inflight = r?.inflightRequests ?? null;
    const poolMax = r?.pool?.max ?? null;
    if (inflight != null && poolMax) {
      if (inflight > poolMax * 4) bump(2, `在途 ${inflight}`);
      else if (inflight > poolMax) bump(1, `在途 ${inflight}`);
    }
    const writing = r?.nginxEdge?.writing ?? null;
    if (writing != null && writing > 100) bump(2, `nginx 积压 ${writing}`);

    return { level, reasons };
  });

  // ── 图表 option ──
  const chartOption = (): EChartsOption => {
    const s = requests()?.series ?? [];
    const labels = s.map((p) => fmtHHMM(p.t));
    return {
      grid: { left: 46, right: 46, top: 18, bottom: 26 },
      tooltip: {
        trigger: 'axis',
        valueFormatter: undefined,
      },
      xAxis: {
        type: 'category',
        data: labels,
        boundaryGap: true,
        axisLabel: { fontSize: 10, color: 'var(--content-tertiary)' },
        axisTick: { show: false },
      },
      yAxis: [
        { type: 'value', name: 'QPS', nameTextStyle: { fontSize: 10 }, axisLabel: { fontSize: 10 } },
        { type: 'value', name: 'P99 ms', nameTextStyle: { fontSize: 10 }, position: 'right', splitLine: { show: false }, axisLabel: { fontSize: 10 } },
      ],
      series: [
        {
          name: 'QPS',
          type: 'bar',
          yAxisIndex: 0,
          data: s.map((p) => Number(p.qps.toFixed(2))),
          itemStyle: { color: '#6366f1', borderRadius: [3, 3, 0, 0] },
          barMaxWidth: 18,
        },
        {
          name: 'P99',
          type: 'line',
          yAxisIndex: 1,
          data: s.map((p) => Math.round(p.p99Ms)),
          smooth: true,
          symbol: 'none',
          lineStyle: { color: '#f59e0b', width: 2 },
          itemStyle: { color: '#f59e0b' },
          areaStyle: { color: 'rgba(245,158,11,0.08)' },
        },
      ],
    };
  };

  return (
    <div class="monitoring">
      <Show when={!loading()} fallback={<div class="flex justify-center py-16"><Spinner size="lg" /></div>}>
        <div class="stack">
          {/* —— 页头 + 工具栏 —— */}
          <div class="page-header">
            <div class="row spread wrap">
              <div>
                <h1 class="page-title">系统监控</h1>
                <p class="page-desc">
                  服务健康 · SLO · {workerStats().total} 个 worker 心跳 · 实时请求与日志。
                  指标随进程生命周期累积，重启清零。
                </p>
              </div>
              <div class="row gap-2 wrap">
                <span
                  class={`chip ${saturation().level === 2 ? 'chip-error' : saturation().level === 1 ? 'chip-warning' : 'chip-success'}`}
                  title={saturation().reasons.length ? `饱和信号:${saturation().reasons.join(' · ')}` : '各项饱和指标均在目标内'}
                >
                  {saturation().level === 2 ? '过载降级' : saturation().level === 1 ? '繁忙' : '健康'}
                  <Show when={saturation().level > 0}>
                    <span class="text-small" style={{ 'margin-left': '4px', opacity: 0.85 }}>· {saturation().reasons[0]}</span>
                  </Show>
                </span>
                <span class="pill-time"><span class="dot" /> 实时 · {clock()}</span>
                <div class="segmented">
                  <For each={WINDOWS}>
                    {(w) => (
                      <button class={window() === w ? 'is-active' : ''} onClick={() => setWindow(w)}>{w}</button>
                    )}
                  </For>
                </div>
                <button
                  class={`btn btn-secondary ${autoRefresh() ? 'is-on' : ''}`}
                  onClick={() => setAutoRefresh((v) => !v)}
                  title={autoRefresh() ? '点击暂停自动刷新' : '点击恢复自动刷新'}
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
                  </svg>
                  {autoRefresh() ? '自动刷新 · 5s' : '已暂停'}
                </button>
              </div>
            </div>
          </div>

          {/* —— SLO 卡 ×4 —— */}
          <Show when={requests()} fallback={<SloPlaceholder />}>
            {(rq) => (
              <div class="grid cols-4">
                {/* 后端可用性:口径=到达后端的请求成功率,边缘失败(TCP/nginx/超时)不计入 */}
                <div class={`slo-card ${toneAvail(rq().availabilityPct)}`}>
                  <div class="l" title="仅统计到达后端的请求的非 5xx 比例;TCP 被拒 / nginx 502 / 客户端超时等边缘失败后端无感知,不计入。系统过载时请看右上角饱和徽标。">后端可用性 <span class="text-small" style={{ opacity: 0.6 }}>ⓘ</span></div>
                  <div class="v">{rq().availabilityPct.toFixed(2)}<span class="unit">%</span></div>
                  <div class="target">目标 ≥ 99.9% · 仅到达后端的请求 · 预算剩余 {Math.max(0, Math.floor(rq().totalRequests * 0.001) - rq().total5xx)} 次</div>
                  <div class="bar"><span style={{ width: `${clamp(rq().availabilityPct, 0, 100)}%` }} /></div>
                </div>
                {/* P50 */}
                <div class={`slo-card ${toneLower(rq().p50Ms, 80)}`}>
                  <div class="l">P50 延迟</div>
                  <div class="v">{Math.round(rq().p50Ms)}<span class="unit">ms</span></div>
                  <div class="target">目标 ≤ 80ms · {window()} 窗口</div>
                  <div class="bar"><span style={{ width: `${clamp((rq().p50Ms / 80) * 100, 3, 100)}%` }} /></div>
                </div>
                {/* P99 */}
                <div class={`slo-card ${toneLower(rq().p99Ms, 400)}`}>
                  <div class="l">P99 延迟</div>
                  <div class="v">{Math.round(rq().p99Ms)}<span class="unit">ms</span></div>
                  <div class="target">目标 ≤ 400ms{p99Spike() ? ` · ★ ${fmtHHMM(p99Spike()!.t)} spike ${Math.round(p99Spike()!.p99Ms)}ms` : ` · ${window()} 窗口`}</div>
                  <div class="bar"><span style={{ width: `${clamp((rq().p99Ms / 400) * 100, 3, 100)}%` }} /></div>
                </div>
                {/* 错误率 */}
                <div class={`slo-card ${toneLower(rq().errorRate * 100, 0.5)}`}>
                  <div class="l">错误率</div>
                  <div class="v">{(rq().errorRate * 100).toFixed(2)}<span class="unit">%</span></div>
                  <div class="target">目标 ≤ 0.5% · {rq().total5xx} / {rq().totalRequests} 次 5xx</div>
                  <div class="bar"><span style={{ width: `${clamp((rq().errorRate * 100 / 0.5) * 100, 3, 100)}%` }} /></div>
                </div>
              </div>
            )}
          </Show>

          {/* —— 服务状态 + worker 心跳 —— */}
          <div class="grid cols-12">
            <div class="panel" style={{ 'grid-column': 'span 5' }}>
              <div class="panel-header">
                <h3>服务状态</h3>
                <span class="aside text-mono text-small">live</span>
              </div>
              <div class="panel-body" style={{ display: 'flex', 'flex-direction': 'column', gap: '8px' }}>
                {/* axum Router */}
                <div class="svc-row is-up">
                  <span class="led" />
                  <div class="name">axum Router <span>HTTP API · /api/* /admin/*</span></div>
                  <div class="row gap-3">
                    <span class="v">{requests() ? `${Math.round(requests()!.qpsAvg)} rps` : '—'}</span>
                    <span class="v">P50 {requests() ? `${Math.round(requests()!.p50Ms)}ms` : '—'}</span>
                    <span class="chip chip-success">healthy</span>
                  </div>
                </div>
                {/* SQLite */}
                <div class={`svc-row ${health()?.status === 'down' ? 'is-down' : 'is-up'}`}>
                  <span class="led" />
                  <div class="name">SQLite (WAL) <span>r2d2 池 · {res()?.pool ? `${res()!.pool!.connections} / ${res()!.pool!.max} 连接` : '—'}</span></div>
                  <div class="row gap-3">
                    <span class="v">{health() ? formatBytes(health()!.dbSizeBytes) : '—'}</span>
                    <span class="v">WAL {res()?.walSizeBytes != null ? formatBytes(res()!.walSizeBytes!) : '—'}</span>
                    <span class={`chip ${health()?.status === 'down' ? 'chip-error' : 'chip-success'}`}>
                      {health()?.status === 'down' ? 'down' : 'healthy'}
                    </span>
                  </div>
                </div>
                {/* AMAS 引擎 */}
                <div class={`svc-row ${health()?.services?.amas.healthy === false ? 'is-warn' : 'is-up'}`}>
                  <span class="led" />
                  <div class="name">AMAS 引擎 <span>config_watcher · llm_advisor · {workerStats().total} jobs</span></div>
                  <div class="row gap-3">
                    <span class="v">{amasTickMs() != null ? `${amasTickMs()!.toFixed(amasTickMs()! < 10 ? 2 : 0)}ms tick` : '—'}</span>
                    <span class="v">{health()?.version ?? '—'}</span>
                    <span class={`chip ${health()?.services?.amas.healthy === false ? 'chip-warning' : 'chip-success'}`}>
                      {health()?.services?.amas.healthy === false ? 'degraded' : 'healthy'}
                    </span>
                  </div>
                </div>
                {/* S2-1:领域事件 outbox 异步消费（默认同步路径时恒 0；RECORDS_OUTBOX_ASYNC=true 后驱动） */}
                <div class={`svc-row ${(health()?.outbox?.deadLetter ?? 0) > 0 ? 'is-warn' : 'is-up'}`}>
                  <span class="led" />
                  <div class="name">事件 outbox <span>records→AMAS 异步消费 · 重试退避 + 死信</span></div>
                  <div class="row gap-3">
                    <span class="v">{health()?.outbox ? `${health()!.outbox!.pending} 待处理` : '—'}</span>
                    <span class="v">{health()?.outbox ? `lag ${health()!.outbox!.lagSecs}s` : '—'}</span>
                    <button
                      type="button"
                      class={`chip ${(health()?.outbox?.deadLetter ?? 0) > 0 ? 'chip-error' : 'chip-success'}`}
                      style={{ cursor: 'pointer', border: 'none' }}
                      title="查看死信明细 / 人工重投 / 丢弃"
                      onClick={openDeadLetter}
                    >
                      {(health()?.outbox?.deadLetter ?? 0) > 0 ? `${health()!.outbox!.deadLetter} 死信` : '无死信'}
                    </button>
                  </div>
                </div>
                {/* Probe 探针 */}
                <div class={`svc-row ${sse()?.healthy === false ? 'is-warn' : 'is-up'}`}>
                  <span class="led" />
                  <div class="name">Probe 探针 <span>ring buffer · {sse() ? `${sse()!.activeConnections}/${sse()!.maxConnections ?? '∞'} 连接` : 'WebSocket SSE'}</span></div>
                  <div class="row gap-3">
                    <span class="v">{sse() ? `${sse()!.activeDevices} 设备` : '—'}</span>
                    <span class="v">flush {fmtAge(workerByName(['probe_cleanup', 'probe_confirm_sweeper'])?.lastRunAt ?? null)}</span>
                    <span class={`chip ${sse()?.healthy === false ? 'chip-warning' : 'chip-success'}`}>
                      {sse()?.healthy === false ? '心跳延迟' : 'healthy'}
                    </span>
                  </div>
                </div>
                {/* DeepSeek LLM 顾问 */}
                <div class="svc-row is-up">
                  <span class="led" />
                  <div class="name">DeepSeek (LLM 顾问) <span>定期巡查 · 白名单约束</span></div>
                  <div class="row gap-3">
                    <span class="v">
                      {advisorCost() ? `¥${advisorCost()!.monthYuan.toFixed(2)} / ${advisorCost()!.monthCapYuan.toFixed(2)}` : '—'}
                    </span>
                    <span class="v">{fmtAge(workerByName(['llm_advisor'])?.lastRunAt ?? null)}</span>
                    <span class="chip chip-success">ok</span>
                  </div>
                </div>
                {/* GitHub Release Checker */}
                <div class="svc-row is-up">
                  <span class="led" />
                  <div class="name">GitHub Release Checker <span>定期轮询 releases</span></div>
                  <div class="row gap-3">
                    <span class="v">
                      {updateCheck() ? `${updateCheck()!.latestVersion} ${updateCheck()!.hasUpdate ? '待安装' : '最新'}` : '—'}
                    </span>
                    <span class="v">{fmtAge(workerByName(['update_checker'])?.lastRunAt ?? null)}</span>
                    <span class={`chip ${updateCheck()?.hasUpdate ? 'chip-info' : 'chip-success'}`}>
                      {updateCheck()?.hasUpdate ? '新版本' : '最新'}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            <div class="panel" style={{ 'grid-column': 'span 7' }}>
              <div class="panel-header">
                <h3>Worker 心跳</h3>
                <span class="aside text-mono text-small">
                  {workerStats().total} jobs · {workerStats().ok} healthy
                  {workerStats().warn > 0 ? ` · ${workerStats().warn} warning` : ''}
                  {workerStats().down > 0 ? ` · ${workerStats().down} error` : ''}
                </span>
              </div>
              <div class="panel-body">
                <Show when={workers().length > 0} fallback={<Empty title="暂无 worker 心跳" description="worker_last_run 表尚无记录" />}>
                  <div class="workers-grid">
                    <For each={workers()}>
                      {(w) => (
                        <div class={`worker-cell ${workerTone(w)}`} title={w.lastError ?? w.lastOutcome ?? ''}>
                          <span class="led" />
                          <span class="label">{w.workerName}</span>
                          <span class="age">{fmtAge(w.lastRunAt)}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </div>
          </div>

          {/* —— 请求延迟图 + 资源条 —— */}
          <div class="grid cols-12">
            <div class="panel" style={{ 'grid-column': 'span 8' }}>
              <div class="panel-header">
                <h3>请求与延迟 · {window()}</h3>
                <span class="text-mono text-small">QPS (bar) + P99 (line)</span>
                <div class="aside">
                  <span class="ministat" style={{ background: 'transparent', padding: '0', 'flex-direction': 'row', 'align-items': 'center', gap: '8px' }}>
                    <span style={{ width: '8px', height: '8px', 'border-radius': '2px', background: '#6366f1' }} />
                    <span class="label">QPS 均值</span>
                    <strong class="tnum">{requests() ? Math.round(requests()!.qpsAvg) : '—'}</strong>
                  </span>
                  <span class="ministat" style={{ background: 'transparent', padding: '0', 'flex-direction': 'row', 'align-items': 'center', gap: '8px' }}>
                    <span style={{ width: '8px', height: '8px', 'border-radius': '2px', background: '#f59e0b' }} />
                    <span class="label">P99 均值</span>
                    <strong class="tnum">{requests() ? `${Math.round(requests()!.p99Ms)}ms` : '—'}</strong>
                  </span>
                </div>
              </div>
              <div class="panel-body">
                <div class="chart">
                  <EChart
                    option={chartOption}
                    height="280px"
                    empty={<div class="flex items-center justify-center h-full text-content-tertiary text-small">该窗口暂无请求样本</div>}
                  />
                </div>
              </div>
            </div>

            <div class="panel" style={{ 'grid-column': 'span 4' }}>
              <div class="panel-header"><h3>资源</h3><span class="aside text-mono text-small">{window()}</span></div>
              <div class="panel-body stack">
                <ResourceBar label="CPU" value={res()?.cpuPct != null ? `${res()!.cpuPct!.toFixed(1)}%` : '—'}
                  pct={res()?.cpuPct ?? 0} tone={res()?.cpuPct != null && res()!.cpuPct! > 85 ? (res()!.cpuPct! > 95 ? 'is-error' : 'is-warning') : ''} />
                <ResourceBar label="内存 RSS"
                  value={res()?.memoryRssBytes != null ? `${formatBytes(res()!.memoryRssBytes!)}${res()?.memoryTotalBytes ? ` / ${formatBytes(res()!.memoryTotalBytes!)}` : ''}` : '—'}
                  pct={res()?.memoryRssBytes != null && res()?.memoryTotalBytes ? (res()!.memoryRssBytes! / res()!.memoryTotalBytes!) * 100 : 0} tone="" />
                <ResourceBar label="DB 连接池"
                  value={res()?.pool ? `${res()!.pool!.connections} / ${res()!.pool!.max}` : '—'}
                  pct={res()?.pool ? (res()!.pool!.connections / Math.max(1, res()!.pool!.max)) * 100 : 0} tone="is-success" />
                <ResourceBar label="磁盘"
                  value={res()?.diskTotalBytes && res()?.diskFreeBytes != null
                    ? `${formatBytes(res()!.diskTotalBytes! - res()!.diskFreeBytes!)} / ${formatBytes(res()!.diskTotalBytes!)}` : '—'}
                  pct={res()?.diskTotalBytes && res()?.diskFreeBytes != null
                    ? ((res()!.diskTotalBytes! - res()!.diskFreeBytes!) / res()!.diskTotalBytes!) * 100 : 0} tone="" />
                <ResourceBar label="SQLite WAL 大小"
                  value={res()?.walSizeBytes != null ? formatBytes(res()!.walSizeBytes!) : '—'}
                  pct={res()?.walSizeBytes != null && health()?.dbSizeBytes ? clamp((res()!.walSizeBytes! / health()!.dbSizeBytes) * 100, 0, 100) : 0}
                  tone={res()?.walSizeBytes != null && health()?.dbSizeBytes && res()!.walSizeBytes! / health()!.dbSizeBytes > 0.5 ? 'is-warning' : ''} />
                <ResourceBar label="入站带宽"
                  value={requests() ? `${(requests()!.bandwidthInBps * 8 / 1e6).toFixed(2)} Mbps` : '—'}
                  pct={requests() ? clamp((requests()!.bandwidthInBps * 8 / 1e6 / 100) * 100, 0, 100) : 0} tone="is-success" />
                {/* 在途请求:过载饱和度直接信号(5xx=0 也能看出堆积) */}
                <ResourceBar label="在途请求 (并发)"
                  value={res()?.inflightRequests != null ? `${res()!.inflightRequests}${res()?.pool?.max ? ` · 池上限 ${res()!.pool!.max}` : ''}` : '—'}
                  pct={res()?.inflightRequests != null && res()?.pool?.max ? clamp((res()!.inflightRequests! / (res()!.pool!.max * 4)) * 100, 0, 100) : 0}
                  tone={res()?.inflightRequests != null && res()?.pool?.max
                    ? (res()!.inflightRequests! > res()!.pool!.max * 4 ? 'is-error' : res()!.inflightRequests! > res()!.pool!.max ? 'is-warning' : '')
                    : ''} />
                {/* nginx 边缘(连接层)——唯一能反映"到达后端之前"的真实压力 */}
                <Show when={res()?.nginxEdge}>
                  {(edge) => (
                    <>
                      <div class="text-small text-content-tertiary" style={{ 'margin-top': '6px', 'border-top': '1px solid var(--border-subtle, #e5e7eb)', 'padding-top': '8px' }}>nginx 边缘 · 连接层</div>
                      <ResourceBar label="活跃连接"
                        value={`${edge().active}`}
                        pct={clamp((edge().active / 1000) * 100, 0, 100)}
                        tone={edge().active > 800 ? 'is-error' : edge().active > 500 ? 'is-warning' : ''} />
                      <ResourceBar label="写中 / 等上游"
                        value={`写 ${edge().writing ?? '—'} · 待 ${edge().waiting ?? '—'}`}
                        pct={clamp(((edge().writing ?? 0) / 200) * 100, 0, 100)}
                        tone={(edge().writing ?? 0) > 100 ? 'is-error' : (edge().writing ?? 0) > 50 ? 'is-warning' : ''} />
                      <ResourceBar label="累计丢弃连接"
                        value={`${edge().dropped}`}
                        pct={edge().dropped > 0 ? 100 : 2}
                        tone={edge().dropped > 0 ? 'is-error' : 'is-success'} />
                    </>
                  )}
                </Show>
              </div>
            </div>
          </div>

          {/* —— 实时日志 + 告警时间线 —— */}
          <div class="grid cols-12">
            <div class="panel" style={{ 'grid-column': 'span 8' }}>
              <div class="panel-header">
                <h3>实时日志</h3>
                <div class="aside">
                  <div class="segmented">
                    <For each={['all', 'INFO', 'WARN', 'ERROR'] as const}>
                      {(lv) => (
                        <button class={logLevel() === lv ? 'is-active' : ''} onClick={() => setLogLevel(lv)}>
                          {lv === 'all' ? '全部' : lv}
                        </button>
                      )}
                    </For>
                  </div>
                  <button class={`btn-icon ${logPaused() ? 'is-on' : ''}`} aria-label={logPaused() ? '恢复' : '暂停'}
                    onClick={() => setLogPaused((v) => !v)}>
                    <Show when={logPaused()} fallback={
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="6" y="4" width="4" height="16" /><rect x="14" y="4" width="4" height="16" /></svg>
                    }>
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3" /></svg>
                    </Show>
                  </button>
                  <button class="btn-icon" aria-label="清空" onClick={() => setLogs([])}>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6" /><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" /></svg>
                  </button>
                </div>
              </div>
              <div class="panel-body" style={{ padding: '14px' }}>
                <div class="log-stream" ref={(el) => (logStreamRef = el)}>
                  <Show when={logs().length > 0} fallback={
                    <div class="text-content-tertiary text-small" style={{ padding: '8px' }}>暂无日志（受日志级别过滤约束）</div>
                  }>
                    <For each={[...logs()].reverse()}>
                      {(line) => (
                        <div class={`log-line lvl-${line.level}`}>
                          <span class="ts">{fmtClockMs(line.tsMs)}</span>
                          <span class="lvl">{line.level}</span>
                          <span class="msg">[{line.target}] {line.message}</span>
                        </div>
                      )}
                    </For>
                  </Show>
                </div>
              </div>
            </div>

            <div class="panel" style={{ 'grid-column': 'span 4' }}>
              <div class="panel-header"><h3>告警时间线</h3><span class="aside text-mono text-small">最近 6h</span></div>
              <div class="panel-body">
                <Show when={events().length > 0} fallback={<Empty title="暂无告警" description="近 6h 无 worker 失败 / 版本更新 / AMAS 异常" />}>
                  <div class="alert-timeline">
                    <For each={events()}>
                      {(e) => (
                        <div class={`alert-item ${e.severity === 'error' ? 'is-error' : e.severity === 'warning' ? 'is-warn' : e.severity === 'resolved' ? 'is-resolved' : 'is-info'}`}>
                          <span class="ts">{fmtHHMM(e.tsMs / 1000)}</span>
                          <span class="marker"><span class="dot" /></span>
                          <div class="body">
                            <strong>{e.title}</strong>
                            <div class="desc">{e.desc}</div>
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </div>
          </div>
        </div>
      </Show>

      {/* W1-2:outbox 死信运维抽屉 —— 明细 + 人工重投 / 丢弃 */}
      <Modal open={dlOpen()} onClose={() => setDlOpen(false)} title="领域事件死信" size="xl">
        <div class="text-sm text-content-secondary mb-4">
          records→AMAS 异步消费重试耗尽 / 永久错误（毒丸）进入死信。可人工重投回 outbox（attempts 归零、立即重新消费）或永久丢弃。
          默认同步路径（RECORDS_OUTBOX_ASYNC=false）下死信恒空。
        </div>
        <Show when={!dlLoading()} fallback={<div style={{ padding: '24px', 'text-align': 'center' }}><Spinner /></div>}>
          <Show when={dlRows().length > 0} fallback={<Empty title="无死信" description="所有领域事件均已成功消费" />}>
            <For each={dlRows()}>
              {(r) => (
                <div style={{
                  display: 'flex', 'align-items': 'center', 'justify-content': 'space-between',
                  gap: '12px', padding: '10px 0', 'border-bottom': '1px solid rgba(255,255,255,0.08)',
                }}>
                  <div style={{ 'min-width': '0', flex: '1' }}>
                    <div class="row gap-3">
                      <span class="chip chip-error">#{r.id}</span>
                      <strong>{r.eventType}</strong>
                      <span class="text-secondary">{r.userId ? `用户 ${r.userId}` : '用户未知'}</span>
                      <span class="text-secondary">· 重试 {r.attempts} 次</span>
                    </div>
                    <div class="text-secondary" style={{ 'font-size': '12px', 'margin-top': '4px' }}>进死信 {r.deadAt}</div>
                    <div style={{
                      'font-size': '12px', 'margin-top': '4px', 'font-family': 'monospace',
                      color: 'var(--color-danger, #f87171)', 'word-break': 'break-all',
                    }}>{r.lastError}</div>
                  </div>
                  <div class="row gap-2" style={{ 'flex-shrink': '0' }}>
                    <Button size="sm" variant="ghost" onClick={() => setDlPending({ action: 'requeue', id: r.id })}>重投</Button>
                    <Button size="sm" variant="danger" onClick={() => setDlPending({ action: 'purge', id: r.id })}>丢弃</Button>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </Show>
      </Modal>

      <ConfirmDialog
        open={dlPending() !== null}
        title={dlPending()?.action === 'requeue' ? '重投死信？' : '丢弃死信？'}
        message={dlPending()?.action === 'requeue'
          ? `事件 #${dlPending()?.id} 将写回 outbox（attempts 归零、立即重新消费）。`
          : `事件 #${dlPending()?.id} 将被永久删除，无法恢复。`}
        variant={dlPending()?.action === 'requeue' ? 'warning' : 'danger'}
        confirmText={dlPending()?.action === 'requeue' ? '重投' : '丢弃'}
        loading={dlBusy()}
        onConfirm={confirmDlAction}
        onCancel={() => setDlPending(null)}
      />
    </div>
  );
}

// ── 子组件 ──
function ResourceBar(props: { label: string; value: string; pct: number; tone: string }) {
  return (
    <div>
      <div class="row spread"><span class="text-small text-secondary">{props.label}</span><strong class="text-mono">{props.value}</strong></div>
      <div class={`progress ${props.tone}`} style={{ 'margin-top': '4px' }}>
        <span style={{ width: `${clamp(props.pct, 0, 100)}%` }} />
      </div>
    </div>
  );
}

function SloPlaceholder() {
  return (
    <div class="grid cols-4">
      <For each={['可用性', 'P50 延迟', 'P99 延迟', '错误率']}>
        {(l) => (
          <div class="slo-card">
            <div class="l">{l}</div>
            <div class="v">—</div>
            <div class="target">等待请求样本…</div>
            <div class="bar"><span style={{ width: '0%' }} /></div>
          </div>
        )}
      </For>
    </div>
  );
}
