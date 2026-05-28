import { createSignal, Show, For, onMount, onCleanup } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { HeroCard } from '@/components/ui/HeroCard';
import { SectionTitle } from '@/components/ui/SectionTitle';
import { WorkerGridCell, type WorkerOutcome } from '@/components/ui/WorkerGridCell';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import { healthApi, type PublicHealthStatus } from '@/api/health';
import { api } from '@/api/http';
import type { SystemHealth, DatabaseInfo } from '@/types/admin';
import type { MonitoringEvent } from '@/types/amas';
import { MONITORING_DEFAULT_LIMIT } from '@/lib/constants';
import { formatBytes } from '@/utils/formatters';

interface WorkerRow {
  workerName: string;
  lastRunAt: string | null;
  lastDurationMs: number | null;
  lastOutcome: string | null;
  lastError: string | null;
}

/** 后端 last_outcome 字符串 → WorkerGridCell 的 4 态枚举 */
function mapOutcome(s: string | null | undefined): WorkerOutcome {
  if (!s) return 'unknown';
  const v = s.toLowerCase();
  if (v === 'ok' || v === 'success' || v === 'completed') return 'ok';
  if (v === 'error' || v === 'failure' || v === 'failed' || v === 'panic') return 'error';
  if (v === 'idle' || v === 'pending' || v === 'scheduled') return 'idle';
  return 'unknown';
}

function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${d}d ${h}h ${m}m`;
}

const statusMap: Record<string, { dot: string; label: string }> = {
  healthy: { dot: 'bg-success', label: '运行正常' },
  degraded: { dot: 'bg-warning', label: '性能降级' },
  down: { dot: 'bg-error', label: '服务异常' },
};

function MetricCell(props: { label: string; children: any }) {
  return (
    <div>
      <p class="text-xs text-content-tertiary">{props.label}</p>
      <p class="text-sm font-medium text-content">{props.children}</p>
    </div>
  );
}

function RecursiveKV(props: { data: unknown; depth?: number }) {
  const depth = props.depth ?? 0;
  if (props.data === null || props.data === undefined) return <span class="text-content-tertiary">—</span>;
  if (typeof props.data !== 'object') return <span>{String(props.data)}</span>;
  if (Array.isArray(props.data)) {
    if (props.data.length === 0) return <span class="text-content-tertiary">[]</span>;
    if (props.data.every(v => typeof v !== 'object')) return <span>{props.data.join(', ')}</span>;
    return <For each={props.data}>{(item) => <RecursiveKV data={item} depth={depth + 1} />}</For>;
  }
  // summary 缩进与 depth 联动；最大 16rem 防止过度缩进
  const summaryPadLeft = `${Math.min(depth, 16)}rem`;
  return (
    <div class={depth > 0 ? 'ml-4 space-y-1' : 'space-y-1'}>
      <For each={Object.entries(props.data as Record<string, unknown>)}>
        {([key, value]) => (
          <Show when={typeof value === 'object' && value !== null}
            fallback={<div class="flex gap-2 text-xs py-0.5" style={{ 'padding-left': summaryPadLeft }}><span class="text-content-tertiary">{key}:</span><span>{String(value)}</span></div>}>
            <details class="text-xs py-0.5">
              <summary class="text-content-tertiary cursor-pointer" style={{ 'padding-left': summaryPadLeft }}>{key}</summary>
              <RecursiveKV data={value} depth={depth + 1} />
            </details>
          </Show>
        )}
      </For>
    </div>
  );
}

const POLL_INTERVAL_MS = 30_000;

export default function MonitoringPage() {
  const [health, setHealth] = createSignal<SystemHealth | null>(null);
  const [publicHealth, setPublicHealth] = createSignal<PublicHealthStatus | null>(null);
  const [db, setDb] = createSignal<DatabaseInfo | null>(null);
  const [monitoring, setMonitoring] = createSignal<MonitoringEvent[] | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [refreshing, setRefreshing] = createSignal(false);
  const [allFailed, setAllFailed] = createSignal(false);
  const [healthErr, setHealthErr] = createSignal('');
  const [publicHealthErr, setPublicHealthErr] = createSignal('');
  const [dbErr, setDbErr] = createSignal('');
  const [monitoringErr, setMonitoringErr] = createSignal('');
  const [lastUpdated, setLastUpdated] = createSignal<Date | null>(null);
  const [workers, setWorkers] = createSignal<WorkerRow[]>([]);
  const [workersErr, setWorkersErr] = createSignal('');

  async function fetchAll() {
    const [h, ph, d, m, w] = await Promise.allSettled([
      adminApi.getHealth(),
      healthApi.getStatus(),
      adminApi.getDatabase(),
      amasApi.getMonitoring(MONITORING_DEFAULT_LIMIT),
      api.get<{ workers: WorkerRow[] }>('/api/admin/monitoring/workers', { useAdminToken: true }),
    ]);
    if (h.status === 'fulfilled') { setHealth(h.value); setHealthErr(''); }
    else setHealthErr(h.reason instanceof Error ? h.reason.message : '加载失败');
    if (ph.status === 'fulfilled') { setPublicHealth(ph.value); setPublicHealthErr(''); }
    else setPublicHealthErr(ph.reason instanceof Error ? ph.reason.message : '加载失败');
    if (d.status === 'fulfilled') { setDb(d.value); setDbErr(''); }
    else setDbErr(d.reason instanceof Error ? d.reason.message : '加载失败');
    if (m.status === 'fulfilled') { setMonitoring(m.value); setMonitoringErr(''); }
    else setMonitoringErr(m.reason instanceof Error ? m.reason.message : '加载失败');
    if (w.status === 'fulfilled') { setWorkers(w.value?.workers ?? []); setWorkersErr(''); }
    else setWorkersErr(w.reason instanceof Error ? w.reason.message : '加载失败');
    const allRejected =
      h.status === 'rejected' && ph.status === 'rejected' && d.status === 'rejected' && m.status === 'rejected';
    setAllFailed(allRejected);
    if (!allRejected) setLastUpdated(new Date());
  }

  async function handleRefresh() {
    if (refreshing()) return;
    setRefreshing(true);
    try { await fetchAll(); } finally { setRefreshing(false); }
  }

  function formatTime(d: Date | null): string {
    if (!d) return '—';
    return d.toLocaleTimeString('zh-CN', { hour12: false });
  }

  onMount(async () => {
    await fetchAll();
    setLoading(false);
    const timer = setInterval(() => { void fetchAll(); }, POLL_INTERVAL_MS);
    onCleanup(() => clearInterval(timer));
  });

  return (
    <div class="space-y-6">
      {/* Hero — SLO 概览 + 全局健康 */}
      <HeroCard
        eyebrow={health() ? (statusMap[health()!.status]?.label ?? '未知') : '检查中…'}
        eyebrowVariant={
          !health() ? 'info'
            : health()!.status === 'healthy' ? 'success'
            : health()!.status === 'degraded' ? 'warning'
            : 'error'
        }
        title="系统监控"
        desc="实时观察 SLO、worker 心跳、数据库状态与 AMAS 监控事件。每 30 秒自动刷新。"
        meta={[
          { value: workers().filter((w) => mapOutcome(w.lastOutcome) === 'ok').length, label: 'Worker OK' },
          { value: workers().filter((w) => mapOutcome(w.lastOutcome) === 'error').length, label: 'Worker Error' },
          { value: health()?.errorRate != null ? `${(health()!.errorRate * 100).toFixed(2)}%` : '—', label: '5xx 错误率' },
          { value: health()?.uptimeSecs != null ? formatUptime(health()!.uptimeSecs) : '—', label: '运行时长' },
        ]}
        cta={<Button size="sm" variant="ghost" loading={refreshing()} onClick={handleRefresh}>刷新</Button>}
      />

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        {/* 顶部 toolbar：上次更新时间戳 */}
        <div class="flex items-center justify-between text-xs">
          <p class="text-content-tertiary">更新于 {formatTime(lastUpdated())}（每 30 秒自动刷新）</p>
          <span class="text-content-tertiary">共 {workers().length} 个 worker</span>
        </div>

        {/* Worker 健康网格 — 21+ tokio::spawn 实时心跳 */}
        <Show when={workers().length > 0} fallback={
          <Show when={workersErr()}>
            <Card variant="outlined"><p class="text-sm text-error">Worker: {workersErr()}</p></Card>
          </Show>
        }>
          <Card variant="elevated">
            <SectionTitle title="Worker 健康网格" sub={`${workers().length} 个 tokio::spawn 心跳`} />
            <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-2.5">
              <For each={workers()}>
                {(w) => (
                  <WorkerGridCell
                    name={w.workerName}
                    outcome={mapOutcome(w.lastOutcome)}
                    lastRunAt={w.lastRunAt}
                    lastDurationMs={w.lastDurationMs}
                    lastError={w.lastError}
                  />
                )}
              </For>
            </div>
          </Card>
        </Show>

        <Show when={!allFailed()} fallback={
          <Empty title="加载失败" description="无法获取任何监控数据，请检查后端服务状态后重试" />
        }>
          {/* System Health */}
          <Show when={health()} fallback={
            <Show when={healthErr()}>
              <Card variant="outlined"><p class="text-sm text-error">系统健康: {healthErr()}</p></Card>
            </Show>
          }>
            {(h) => {
              const st = () => statusMap[h().status] ?? statusMap.down;
              return (
                <Card variant="elevated">
                  <h2 class="text-headline text-content mb-3">系统健康</h2>
                  <div class="grid grid-cols-2 lg:grid-cols-4 gap-4 text-sm">
                    <MetricCell label="状态">
                      <span class="flex items-center gap-1.5">
                        <span class={`w-2 h-2 rounded-full ${st().dot}`} />
                        {st().label}
                      </span>
                    </MetricCell>
                    <MetricCell label="版本">{h().version}</MetricCell>
                    <MetricCell label="运行时间">{formatUptime(h().uptimeSecs)}</MetricCell>
                    <MetricCell label="数据库大小">{formatBytes(h().dbSizeBytes)}</MetricCell>
                    <MetricCell label="5xx 错误率">{(h().errorRate * 100).toFixed(2)}%</MetricCell>
                  </div>
                </Card>
              );
            }}
          </Show>

          {/* Public Health Probe */}
          <Show when={publicHealth()} fallback={
            <Show when={publicHealthErr()}>
              <Card variant="outlined"><p class="text-sm text-error">公开健康探针: {publicHealthErr()}</p></Card>
            </Show>
          }>
            {(ph) => (
              <Card variant="elevated">
                <h2 class="text-headline text-content mb-3">公开健康探针</h2>
                <div class="flex items-center justify-between gap-2 mb-3 flex-wrap">
                  <Badge variant={ph().status === 'ok' ? 'success' : 'error'}>{ph().status}</Badge>
                  <span class="text-xs text-content-tertiary">更新于 {formatTime(lastUpdated())}</span>
                </div>
                <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
                  <For each={Object.entries(ph().services)}>
                    {([name, svc]) => (
                      <div class="flex items-center gap-2 text-sm">
                        <Badge variant={(svc as any).healthy ? 'success' : 'error'} size="sm">
                          {(svc as any).healthy ? '正常' : '异常'}
                        </Badge>
                        <span class="text-content-secondary">{name}</span>
                      </div>
                    )}
                  </For>
                </div>
              </Card>
            )}
          </Show>

          {/* Database Info */}
          <Show when={db()} fallback={
            <Show when={dbErr()}>
              <Card variant="outlined"><p class="text-sm text-error">数据库信息: {dbErr()}</p></Card>
            </Show>
          }>
            {(d) => (
              <Card variant="elevated">
                <h2 class="text-headline text-content mb-3">数据库信息</h2>
                <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-4">
                  <MetricCell label="大小">{formatBytes(d().sizeOnDisk)}</MetricCell>
                  <MetricCell label="表数量">{d().tableCount}</MetricCell>
                  <MetricCell label="页大小">{d().pageSize} bytes</MetricCell>
                  <MetricCell label="页数量">{d().pageCount}</MetricCell>
                  <MetricCell label="WAL 模式">
                    <Badge variant={d().walEnabled ? 'success' : 'warning'} size="sm">
                      {d().walEnabled ? '启用' : '未启用'}
                    </Badge>
                  </MetricCell>
                </div>
              </Card>
            )}
          </Show>

          {/* AMAS Monitoring Events */}
          <Show when={monitoring()} fallback={
            <Show when={monitoringErr()}>
              <Card variant="outlined"><p class="text-sm text-error">AMAS 监控: {monitoringErr()}</p></Card>
            </Show>
          }>
            {(events) => (
              <Card variant="elevated">
                <h2 class="text-headline text-content mb-3">AMAS 监控事件</h2>
                <Show when={events().length > 0} fallback={<Empty title="暂无 AMAS 监控事件" description="近期没有可显示的监控事件" />}>
                  <div class="max-h-96 overflow-y-auto space-y-2">
                    <For each={events()}>
                      {(event) => (
                        <Card variant="outlined" padding="sm">
                          <div class="flex items-center gap-2 mb-2 flex-wrap">
                            <span class="text-xs text-content-tertiary tabular-nums">{new Date(event.timestamp).toLocaleString()}</span>
                            <Badge variant="accent" size="sm">{event.eventType}</Badge>
                          </div>
                          <RecursiveKV data={event.data} />
                        </Card>
                      )}
                    </For>
                  </div>
                </Show>
              </Card>
            )}
          </Show>
        </Show>
      </Show>
    </div>
  );
}
