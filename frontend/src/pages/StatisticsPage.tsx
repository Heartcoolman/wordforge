import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { ProgressBar } from '@/components/ui/Progress';
import { uiStore } from '@/stores/ui';
import { usersApi } from '@/api/users';
import { recordsApi } from '@/api/records';
import { amasApi } from '@/api/amas';
import { wordStatesApi } from '@/api/wordStates';
import type { UserStats } from '@/types/user';
import type { EnhancedStatistics } from '@/types/record';
import type { AmasUserState } from '@/types/amas';
import type { WordStateOverview } from '@/types/wordState';
import { formatPercent, formatNumber } from '@/utils/formatters';

export default function StatisticsPage() {
  const [stats, setStats] = createSignal<UserStats | null>(null);
  const [enhanced, setEnhanced] = createSignal<EnhancedStatistics | null>(null);
  const [amasState, setAmasState] = createSignal<AmasUserState | null>(null);
  const [wordOverview, setWordOverview] = createSignal<WordStateOverview | null>(null);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    try {
      const [s, e, a, w] = await Promise.allSettled([
        usersApi.getStats(),
        recordsApi.enhancedStatistics(),
        amasApi.getState(),
        wordStatesApi.getOverview(),
      ]);
      if (s.status === 'fulfilled') setStats(s.value);
      if (e.status === 'fulfilled') setEnhanced(e.value);
      if (a.status === 'fulfilled') setAmasState(a.value);
      if (w.status === 'fulfilled') setWordOverview(w.value);
    } catch (err: unknown) {
      uiStore.toast.error('加载统计失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">学习统计</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        {/* Overview Cards */}
        <Show when={stats()}>
          {(s) => (
            <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
              <StatCard label="学习单词" value={formatNumber(s().totalWordsLearned)} icon="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" color="accent" />
              <StatCard label="总记录数" value={formatNumber(s().totalRecords)} icon="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" color="info" />
              <StatCard label="连续天数" value={`${s().streakDays} 天`} icon="M17.657 18.657A8 8 0 016.343 7.343S7 9 9 10c0-2 .5-5 2.986-7C14 5 16.09 5.777 17.656 7.343A7.975 7.975 0 0120 13a7.975 7.975 0 01-2.343 5.657z" color="warning" />
              <StatCard label="正确率" value={formatPercent(s().accuracyRate)} icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" color="success" />
            </div>
          )}
        </Show>

        {/* Word State Distribution */}
        <Show when={wordOverview()}>
          {(wo) => {
            const total = () => wo().new + wo().learning + wo().reviewing + wo().mastered;
            return (
              <Card variant="elevated">
                <h2 class="text-lg font-semibold text-content mb-4">单词状态分布</h2>
                <div class="grid grid-cols-4 gap-4 mb-4">
                  <div class="text-center">
                    <p class="text-xl font-bold text-content-tertiary">{wo().new}</p>
                    <p class="text-xs text-content-secondary">新单词</p>
                  </div>
                  <div class="text-center">
                    <p class="text-xl font-bold text-info">{wo().learning}</p>
                    <p class="text-xs text-content-secondary">学习中</p>
                  </div>
                  <div class="text-center">
                    <p class="text-xl font-bold text-warning">{wo().reviewing}</p>
                    <p class="text-xs text-content-secondary">复习中</p>
                  </div>
                  <div class="text-center">
                    <p class="text-xl font-bold text-success">{wo().mastered}</p>
                    <p class="text-xs text-content-secondary">已掌握</p>
                  </div>
                </div>
                <Show when={total() > 0}>
                  <div class="flex rounded-full overflow-hidden h-3">
                    <div class="bg-surface-tertiary" style={{ width: `${(wo().new / total()) * 100}%` }} />
                    <div class="bg-info" style={{ width: `${(wo().learning / total()) * 100}%` }} />
                    <div class="bg-warning" style={{ width: `${(wo().reviewing / total()) * 100}%` }} />
                    <div class="bg-success" style={{ width: `${(wo().mastered / total()) * 100}%` }} />
                  </div>
                </Show>
              </Card>
            );
          }}
        </Show>

        {/* AMAS State Radar */}
        <Show when={amasState()}>
          {(state) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-4">认知状态</h2>
              <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                <StateBar label="注意力" value={state().attention} color="accent" />
                <StateBar label="疲劳度" value={state().fatigue} color="error" />
                <StateBar label="动机" value={state().motivation} color="success" />
                <StateBar label="信心" value={state().confidence} color="info" />
              </div>
              <p class="text-xs text-content-tertiary mt-3">总事件数: {state().totalEventCount} | 会话事件: {state().sessionEventCount}</p>
            </Card>
          )}
        </Show>

        {/* Daily Chart */}
        <Show when={enhanced()?.daily && enhanced()!.daily.length > 0}>
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-4">每日学习</h2>
            <div class="space-y-2">
              <For each={enhanced()!.daily.slice(-14)}>
                {(day) => (
                  <div class="flex items-center gap-3 text-sm">
                    <span class="w-20 text-content-secondary text-xs">{day.date}</span>
                    <div class="flex-1"><ProgressBar value={day.total} max={Math.max(...enhanced()!.daily.map((d) => d.total), 1)} size="sm" color={day.accuracy > 0.8 ? 'success' : day.accuracy > 0.5 ? 'warning' : 'error'} /></div>
                    <span class="w-12 text-right text-xs text-content-tertiary">{day.total} 题</span>
                    <span class="w-12 text-right text-xs text-content-tertiary">{(day.accuracy * 100).toFixed(0)}%</span>
                  </div>
                )}
              </For>
            </div>
          </Card>
        </Show>
      </Show>
    </div>
  );
}

function StatCard(props: { label: string; value: string; icon: string; color: string }) {
  const bgMap: Record<string, string> = { accent: 'bg-accent-light', success: 'bg-success-light', warning: 'bg-warning-light', info: 'bg-info-light', error: 'bg-error-light' };
  const textMap: Record<string, string> = { accent: 'text-accent', success: 'text-success', warning: 'text-warning', info: 'text-info', error: 'text-error' };
  return (
    <Card variant="elevated" padding="md">
      <div class="flex items-center gap-3">
        <div class={`w-10 h-10 rounded-xl flex items-center justify-center ${bgMap[props.color]}`}>
          <svg class={`w-5 h-5 ${textMap[props.color]}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d={props.icon} />
          </svg>
        </div>
        <div>
          <p class="text-xl font-bold text-content">{props.value}</p>
          <p class="text-xs text-content-secondary">{props.label}</p>
        </div>
      </div>
    </Card>
  );
}

function StateBar(props: { label: string; value: number; color: string }) {
  return (
    <div>
      <div class="flex justify-between text-xs mb-1">
        <span class="text-content-secondary">{props.label}</span>
        <span class="text-content">{(props.value * 100).toFixed(0)}%</span>
      </div>
      <ProgressBar value={props.value * 100} max={100} size="sm" color={props.color as 'accent'} />
    </div>
  );
}
