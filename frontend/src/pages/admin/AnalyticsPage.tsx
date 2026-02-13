import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { formatNumber, formatPercent } from '@/utils/formatters';

export default function AnalyticsPage() {
  const [engagement, setEngagement] = createSignal<{ totalUsers: number; activeToday: number; retentionRate: number } | null>(null);
  const [learning, setLearning] = createSignal<{ totalWords: number; totalRecords: number; totalCorrect: number; overallAccuracy: number } | null>(null);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    const [e, l] = await Promise.allSettled([
      adminApi.getEngagement(),
      adminApi.getLearningAnalytics(),
    ]);
    if (e.status === 'fulfilled') setEngagement(e.value);
    if (l.status === 'fulfilled') setLearning(l.value);
    // 部分失败时显示 toast 提示
    if (e.status === 'rejected' && l.status === 'fulfilled') {
      uiStore.toast.warning('活跃度数据加载失败', e.reason instanceof Error ? e.reason.message : '');
    } else if (l.status === 'rejected' && e.status === 'fulfilled') {
      uiStore.toast.warning('学习数据加载失败', l.reason instanceof Error ? l.reason.message : '');
    }
    setLoading(false);
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">数据分析</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={engagement()}>
          {(e) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-4">用户活跃度</h2>
              <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                <div class="text-center">
                  <p class="text-2xl font-bold text-accent">{formatNumber(e().totalUsers)}</p>
                  <p class="text-xs text-content-secondary">总用户</p>
                </div>
                <div class="text-center">
                  <p class="text-2xl font-bold text-success">{formatNumber(e().activeToday)}</p>
                  <p class="text-xs text-content-secondary">今日活跃</p>
                </div>
                <div class="text-center">
                  <p class="text-2xl font-bold text-info">{formatPercent(e().retentionRate)}</p>
                  <p class="text-xs text-content-secondary">日活跃率</p>
                </div>
              </div>
            </Card>
          )}
        </Show>

        <Show when={learning()}>
          {(l) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-4">学习数据</h2>
              <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div class="text-center">
                  <p class="text-2xl font-bold text-accent">{formatNumber(l().totalWords)}</p>
                  <p class="text-xs text-content-secondary">总单词</p>
                </div>
                <div class="text-center">
                  <p class="text-2xl font-bold text-info">{formatNumber(l().totalRecords)}</p>
                  <p class="text-xs text-content-secondary">总记录</p>
                </div>
                <div class="text-center">
                  <p class="text-2xl font-bold text-success">{formatNumber(l().totalCorrect)}</p>
                  <p class="text-xs text-content-secondary">正确数</p>
                </div>
                <div class="text-center">
                  <p class="text-2xl font-bold text-warning">{formatPercent(l().overallAccuracy)}</p>
                  <p class="text-xs text-content-secondary">总正确率</p>
                </div>
              </div>
            </Card>
          )}
        </Show>
      </Show>
    </div>
  );
}
