import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { formatNumber } from '@/utils/formatters';

export default function AdminDashboard() {
  const [stats, setStats] = createSignal<{ users: number; words: number; records: number } | null>(null);
  const [health, setHealth] = createSignal<{ status: string; dbSizeBytes: number; uptimeSecs: string | number; version: string } | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal(false);

  onMount(async () => {
    const [s, h] = await Promise.allSettled([adminApi.getStats(), adminApi.getHealth()]);
    if (s.status === 'fulfilled') setStats(s.value);
    if (h.status === 'fulfilled') setHealth(h.value);
    // 部分失败时显示 toast 提示
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
      <h1 class="text-2xl font-bold text-content">仪表盘</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={!error() || stats() || health()} fallback={
          <Empty title="加载失败" description="无法获取仪表盘数据，请稍后重试" />
        }>
        <Show when={stats()}>
          {(s) => (
            <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
              <Card variant="elevated" padding="lg">
                <p class="text-3xl font-bold text-accent">{formatNumber(s().users)}</p>
                <p class="text-sm text-content-secondary">注册用户</p>
              </Card>
              <Card variant="elevated" padding="lg">
                <p class="text-3xl font-bold text-info">{formatNumber(s().words)}</p>
                <p class="text-sm text-content-secondary">单词总数</p>
              </Card>
              <Card variant="elevated" padding="lg">
                <p class="text-3xl font-bold text-success">{formatNumber(s().records)}</p>
                <p class="text-sm text-content-secondary">学习记录</p>
              </Card>
            </div>
          )}
        </Show>

        <Show when={health()}>
          {(h) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-3">系统状态</h2>
              <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                <div><p class="text-content-secondary">状态</p><p class="font-medium text-success">{h().status}</p></div>
                <div><p class="text-content-secondary">数据库大小</p><p class="font-medium text-content">{(h().dbSizeBytes / 1024 / 1024).toFixed(2)} MB</p></div>
                <div><p class="text-content-secondary">运行时间</p><p class="font-medium text-content">{typeof h().uptimeSecs === 'number' ? `${Math.floor(h().uptimeSecs as number / 3600)} 小时` : String(h().uptimeSecs)}</p></div>
                <div><p class="text-content-secondary">版本</p><p class="font-medium text-content">{h().version}</p></div>
              </div>
            </Card>
          )}
        </Show>
        </Show>
      </Show>
    </div>
  );
}
