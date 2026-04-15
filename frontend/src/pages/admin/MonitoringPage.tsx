import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import { healthApi, type PublicHealthStatus } from '@/api/health';
import type { SystemHealth, DatabaseInfo } from '@/types/admin';
import type { MonitoringEvent } from '@/types/amas';
import { MONITORING_DEFAULT_LIMIT } from '@/lib/constants';

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
    return <For each={props.data}>{(item, i) => <RecursiveKV data={item} depth={depth + 1} />}</For>;
  }
  return (
    <div class={depth > 0 ? 'ml-4' : ''}>
      <For each={Object.entries(props.data as Record<string, unknown>)}>
        {([key, value]) => (
          <Show when={typeof value === 'object' && value !== null}
            fallback={<div class="flex gap-2 text-xs py-0.5"><span class="text-content-tertiary">{key}:</span><span>{String(value)}</span></div>}>
            <details class="text-xs py-0.5">
              <summary class="text-content-tertiary cursor-pointer">{key}</summary>
              <RecursiveKV data={value} depth={depth + 1} />
            </details>
          </Show>
        )}
      </For>
    </div>
  );
}

export default function MonitoringPage() {
  const [health, setHealth] = createSignal<SystemHealth | null>(null);
  const [publicHealth, setPublicHealth] = createSignal<PublicHealthStatus | null>(null);
  const [db, setDb] = createSignal<DatabaseInfo | null>(null);
  const [monitoring, setMonitoring] = createSignal<MonitoringEvent[] | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [allFailed, setAllFailed] = createSignal(false);
  const [healthErr, setHealthErr] = createSignal('');
  const [publicHealthErr, setPublicHealthErr] = createSignal('');
  const [dbErr, setDbErr] = createSignal('');
  const [monitoringErr, setMonitoringErr] = createSignal('');

  onMount(async () => {
    const [h, ph, d, m] = await Promise.allSettled([
      adminApi.getHealth(),
      healthApi.getStatus(),
      adminApi.getDatabase(),
      amasApi.getMonitoring(MONITORING_DEFAULT_LIMIT),
    ]);
    if (h.status === 'fulfilled') setHealth(h.value);
    else setHealthErr(h.reason instanceof Error ? h.reason.message : '加载失败');
    if (ph.status === 'fulfilled') setPublicHealth(ph.value);
    else setPublicHealthErr(ph.reason instanceof Error ? ph.reason.message : '加载失败');
    if (d.status === 'fulfilled') setDb(d.value);
    else setDbErr(d.reason instanceof Error ? d.reason.message : '加载失败');
    if (m.status === 'fulfilled') setMonitoring(m.value);
    else setMonitoringErr(m.reason instanceof Error ? m.reason.message : '加载失败');
    if (h.status === 'rejected' && ph.status === 'rejected' && d.status === 'rejected' && m.status === 'rejected') {
      setAllFailed(true);
    }
    setLoading(false);
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
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
                  <h2 class="text-lg font-semibold text-content mb-3">系统健康</h2>
                  <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                    <MetricCell label="状态">
                      <span class="flex items-center gap-1.5">
                        <span class={`w-2 h-2 rounded-full ${st().dot}`} />
                        {st().label}
                      </span>
                    </MetricCell>
                    <MetricCell label="版本">{h().version}</MetricCell>
                    <MetricCell label="运行时间">{formatUptime(h().uptimeSecs)}</MetricCell>
                    <MetricCell label="数据库大小">{(h().dbSizeBytes / 1024 / 1024).toFixed(2)} MB</MetricCell>
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
                <h2 class="text-lg font-semibold text-content mb-3">公开健康探针</h2>
                <div class="flex items-center gap-2 mb-3">
                  <Badge variant={ph().status === 'ok' ? 'success' : 'error'}>{ph().status}</Badge>
                </div>
                <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
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
                <h2 class="text-lg font-semibold text-content mb-3">数据库信息</h2>
                <div class="grid grid-cols-2 md:grid-cols-3 gap-4">
                  <MetricCell label="大小">{(d().sizeOnDisk / 1024 / 1024).toFixed(2)} MB</MetricCell>
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
                <h2 class="text-lg font-semibold text-content mb-3">AMAS 监控事件</h2>
                <Show when={events().length > 0} fallback={<p class="text-sm text-content-secondary">暂无事件</p>}>
                  <div class="max-h-96 overflow-y-auto space-y-2">
                    <For each={events()}>
                      {(event) => (
                        <div class="border border-border/50 rounded-lg p-3">
                          <div class="flex items-center gap-2 mb-2">
                            <span class="text-xs text-content-tertiary">{new Date(event.timestamp).toLocaleString()}</span>
                            <Badge variant="accent" size="sm">{event.eventType}</Badge>
                          </div>
                          <RecursiveKV data={event.data} />
                        </div>
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
