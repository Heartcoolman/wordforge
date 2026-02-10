import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import type { SystemHealth, DatabaseInfo } from '@/types/admin';
import type { MonitoringEvent } from '@/api/amas';

export default function MonitoringPage() {
  const [health, setHealth] = createSignal<SystemHealth | null>(null);
  const [db, setDb] = createSignal<DatabaseInfo | null>(null);
  const [monitoring, setMonitoring] = createSignal<MonitoringEvent[] | null>(null);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    try {
      const [h, d, m] = await Promise.allSettled([
        adminApi.getHealth(),
        adminApi.getDatabase(),
        amasApi.getMonitoring(20),
      ]);
      if (h.status === 'fulfilled') setHealth(h.value);
      if (d.status === 'fulfilled') setDb(d.value);
      if (m.status === 'fulfilled') setMonitoring(m.value);
    } catch { /* ignore */ }
    setLoading(false);
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">系统监控</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={health()}>
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-3">系统健康</h2>
            <pre class="text-xs font-mono text-content-secondary bg-surface-secondary p-4 rounded-lg overflow-x-auto">
              {JSON.stringify(health(), null, 2)}
            </pre>
          </Card>
        </Show>

        <Show when={db()}>
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-3">数据库信息</h2>
            <pre class="text-xs font-mono text-content-secondary bg-surface-secondary p-4 rounded-lg overflow-x-auto">
              {JSON.stringify(db(), null, 2)}
            </pre>
          </Card>
        </Show>

        <Show when={monitoring()}>
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-3">AMAS 监控事件</h2>
            <pre class="text-xs font-mono text-content-secondary bg-surface-secondary p-4 rounded-lg overflow-x-auto max-h-96">
              {JSON.stringify(monitoring(), null, 2)}
            </pre>
          </Card>
        </Show>
      </Show>
    </div>
  );
}
