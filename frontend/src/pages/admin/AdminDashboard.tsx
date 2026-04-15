import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { StatCard } from '@/components/ui/StatCard';
import { EChart } from '@/components/ui/EChart';
import { Skeleton } from '@/components/ui/Skeleton';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { formatNumber } from '@/utils/formatters';
import type { AdminStats, UpdateCheck, DailyActiveUsersEntry } from '@/types/admin';

export default function AdminDashboard() {
  const [stats, setStats] = createSignal<AdminStats | null>(null);
  const [health, setHealth] = createSignal<{ status: string; dbSizeBytes: number; uptimeSecs: string | number; version: string } | null>(null);
  const [updateInfo, setUpdateInfo] = createSignal<UpdateCheck | null>(null);
  const [dailyUsers, setDailyUsers] = createSignal<DailyActiveUsersEntry[] | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal(false);

  onMount(async () => {
    const [s, h, u, dau] = await Promise.allSettled([
      adminApi.getStats(), adminApi.getHealth(), adminApi.checkUpdate(), adminApi.getDailyActiveUsers(7),
    ]);
    if (s.status === 'fulfilled') setStats(s.value);
    if (h.status === 'fulfilled') setHealth(h.value);
    if (u.status === 'fulfilled') setUpdateInfo(u.value);
    if (dau.status === 'fulfilled') setDailyUsers(dau.value);
    if (s.status === 'rejected' && h.status === 'fulfilled') {
      uiStore.toast.warning('统计数据加载失败', s.reason instanceof Error ? s.reason.message : '');
    } else if (h.status === 'rejected' && s.status === 'fulfilled') {
      uiStore.toast.warning('健康状态加载失败', h.reason instanceof Error ? h.reason.message : '');
    }
    if (s.status === 'rejected' && h.status === 'rejected') setError(true);
    setLoading(false);
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={!error() || stats() || health()} fallback={
          <Empty title="加载失败" description="无法获取仪表盘数据，请稍后重试" />
        }>
        <Show when={stats()}>
          {(s) => (
            <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
              <StatCard title="注册用户" value={formatNumber(s().users)} color="accent"
                icon="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z"
                trend={s().trend?.users} />
              <StatCard title="单词总数" value={formatNumber(s().words)} color="info"
                icon="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
              <StatCard title="学习记录" value={formatNumber(s().records)} color="success"
                icon="M16 8v8m-4-5v5m-4-2v2m-2 4h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
                trend={s().trend?.records} />
            </div>
          )}
        </Show>

        <Show when={health()}>
          {(h) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-3">系统状态</h2>
              <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                <div>
                  <p class="text-content-secondary">状态</p>
                  <p class="font-medium flex items-center gap-1.5">
                    <span class={`w-2 h-2 rounded-full ${h().status === 'healthy' ? 'bg-success' : h().status === 'degraded' ? 'bg-warning' : 'bg-error'}`} />
                    <span class={h().status === 'healthy' ? 'text-success' : h().status === 'degraded' ? 'text-warning' : 'text-error'}>
                      {h().status === 'healthy' ? '运行正常' : h().status === 'degraded' ? '性能降级' : '服务异常'}
                    </span>
                  </p>
                </div>
                <div><p class="text-content-secondary">数据库大小</p><p class="font-medium text-content">{(h().dbSizeBytes / 1024 / 1024).toFixed(2)} MB</p></div>
                <div>
                  <p class="text-content-secondary">运行时间</p>
                  <p class="font-medium text-content">{(() => {
                    const sec = typeof h().uptimeSecs === 'number' ? h().uptimeSecs as number : 0;
                    const d = Math.floor(sec / 86400);
                    const hr = Math.floor((sec % 86400) / 3600);
                    const m = Math.floor((sec % 3600) / 60);
                    return `${d}d ${hr}h ${m}m`;
                  })()}</p>
                </div>
                <div>
                  <p class="text-content-secondary">版本</p>
                  <div class="flex items-center gap-2">
                    <p class="font-medium text-content">{h().version}</p>
                    <Show when={updateInfo()?.hasUpdate && updateInfo()?.releaseUrl}>
                      <a
                        href={updateInfo()!.releaseUrl!}
                        target="_blank"
                        rel="noopener noreferrer"
                        class="text-xs px-1.5 py-0.5 rounded bg-accent/10 text-accent hover:bg-accent/20 transition-colors"
                      >
                        新版本 {updateInfo()!.latestVersion}
                      </a>
                    </Show>
                  </div>
                </div>
              </div>
            </Card>
          )}
        </Show>

        <Card variant="elevated">
          <h2 class="text-lg font-semibold text-content mb-3">日活跃用户趋势</h2>
          <Show when={dailyUsers()} fallback={<Skeleton height="320px" />}>
            {(data) => (
              <EChart option={() => {
                const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#6366f1';
                return {
                  grid: { left: 40, right: 20, top: 20, bottom: 30 },
                  xAxis: { type: 'category' as const, data: data().map(d => d.date.slice(5)) },
                  yAxis: { type: 'value' as const, minInterval: 1 },
                  tooltip: { trigger: 'axis' as const },
                  series: [{ type: 'line' as const, data: data().map(d => d.count), smooth: true, itemStyle: { color: accent }, areaStyle: { opacity: 0.1 } }],
                };
              }} />
            )}
          </Show>
        </Card>
        </Show>
      </Show>
    </div>
  );
}
