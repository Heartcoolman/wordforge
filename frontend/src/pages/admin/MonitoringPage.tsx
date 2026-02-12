import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import type { SystemHealth, DatabaseInfo } from '@/types/admin';
import type { MonitoringEvent } from '@/types/amas';
import { MONITORING_DEFAULT_LIMIT } from '@/lib/constants';

/** 过滤展示数据中的敏感字段 */
const SENSITIVE_KEYS = new Set([
  'password', 'passwordHash', 'password_hash', 'secret', 'token',
  'accessToken', 'refreshToken', 'apiKey', 'api_key', 'authorization',
]);

function filterSensitiveFields(obj: unknown): unknown {
  if (obj === null || obj === undefined || typeof obj !== 'object') return obj;
  if (Array.isArray(obj)) return obj.map(filterSensitiveFields);
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
    if (SENSITIVE_KEYS.has(key)) {
      result[key] = '[REDACTED]';
    } else {
      result[key] = filterSensitiveFields(value);
    }
  }
  return result;
}

export default function MonitoringPage() {
  const [health, setHealth] = createSignal<SystemHealth | null>(null);
  const [db, setDb] = createSignal<DatabaseInfo | null>(null);
  const [monitoring, setMonitoring] = createSignal<MonitoringEvent[] | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [allFailed, setAllFailed] = createSignal(false);
  const [healthErr, setHealthErr] = createSignal('');
  const [dbErr, setDbErr] = createSignal('');
  const [monitoringErr, setMonitoringErr] = createSignal('');

  onMount(async () => {
    const [h, d, m] = await Promise.allSettled([
      adminApi.getHealth(),
      adminApi.getDatabase(),
      amasApi.getMonitoring(MONITORING_DEFAULT_LIMIT),
    ]);
    if (h.status === 'fulfilled') setHealth(h.value);
    else setHealthErr(h.reason instanceof Error ? h.reason.message : '加载失败');
    if (d.status === 'fulfilled') setDb(d.value);
    else setDbErr(d.reason instanceof Error ? d.reason.message : '加载失败');
    if (m.status === 'fulfilled') setMonitoring(m.value);
    else setMonitoringErr(m.reason instanceof Error ? m.reason.message : '加载失败');
    if (h.status === 'rejected' && d.status === 'rejected' && m.status === 'rejected') {
      setAllFailed(true);
    }
    setLoading(false);
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">系统监控</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={!allFailed()} fallback={
          <Empty title="加载失败" description="无法获取任何监控数据，请检查后端服务状态后重试" />
        }>
          <Show when={health()} fallback={
            <Show when={healthErr()}>
              <Card variant="outlined"><p class="text-sm text-error">系统健康: {healthErr()}</p></Card>
            </Show>
          }>
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-3">系统健康</h2>
              <pre class="text-xs font-mono text-content-secondary bg-surface-secondary p-4 rounded-lg overflow-x-auto">
                {JSON.stringify(filterSensitiveFields(health()), null, 2)}
              </pre>
            </Card>
          </Show>

          <Show when={db()} fallback={
            <Show when={dbErr()}>
              <Card variant="outlined"><p class="text-sm text-error">数据库信息: {dbErr()}</p></Card>
            </Show>
          }>
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-3">数据库信息</h2>
              <pre class="text-xs font-mono text-content-secondary bg-surface-secondary p-4 rounded-lg overflow-x-auto">
                {JSON.stringify(filterSensitiveFields(db()), null, 2)}
              </pre>
            </Card>
          </Show>

          <Show when={monitoring()} fallback={
            <Show when={monitoringErr()}>
              <Card variant="outlined"><p class="text-sm text-error">AMAS 监控: {monitoringErr()}</p></Card>
            </Show>
          }>
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-3">AMAS 监控事件</h2>
              <pre class="text-xs font-mono text-content-secondary bg-surface-secondary p-4 rounded-lg overflow-x-auto max-h-96">
                {JSON.stringify(filterSensitiveFields(monitoring()), null, 2)}
              </pre>
            </Card>
          </Show>
        </Show>
      </Show>
    </div>
  );
}
