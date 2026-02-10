import { createSignal, Show, onMount } from 'solid-js';
import { A } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { ProgressBar, CircularProgress } from '@/components/ui/Progress';
import { Spinner } from '@/components/ui/Spinner';
import { authStore } from '@/stores/auth';
import { uiStore } from '@/stores/ui';
import { studyConfigApi } from '@/api/studyConfig';
import { usersApi } from '@/api/users';
import type { StudyProgress } from '@/types/studyConfig';
import type { UserStats } from '@/types/user';
import { formatNumber, formatPercent } from '@/utils/formatters';

export default function HomePage() {
  return (
    <Show when={authStore.isAuthenticated()} fallback={<WelcomePage />}>
      <Dashboard />
    </Show>
  );
}

function WelcomePage() {
  return (
    <div class="text-center py-16 animate-fade-in-up">
      <div class="w-20 h-20 mx-auto mb-6 rounded-2xl bg-accent-light flex items-center justify-center">
        <svg class="w-10 h-10 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
        </svg>
      </div>
      <h1 class="text-4xl font-bold text-content mb-3">WordMaster</h1>
      <p class="text-lg text-content-secondary mb-8 max-w-md mx-auto">
        智能英语词汇学习平台，基于 AMAS 自适应算法，让记忆更高效
      </p>
      <div class="flex gap-4 justify-center flex-wrap">
        <Feature icon="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" title="智能策略" desc="AMAS 算法动态调整难度" />
        <Feature icon="M13 10V3L4 14h7v7l9-11h-7z" title="高效学习" desc="间隔重复科学记忆" />
        <Feature icon="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" title="数据洞察" desc="全面的学习数据统计" />
      </div>
      <div class="mt-10 flex gap-3 justify-center">
        <A href="/login"><Button variant="outline" size="lg">登录</Button></A>
        <A href="/register"><Button size="lg">开始学习</Button></A>
      </div>
    </div>
  );
}

function Feature(props: { icon: string; title: string; desc: string }) {
  return (
    <Card variant="outlined" padding="md" class="w-56 text-center">
      <svg class="w-8 h-8 mx-auto text-accent mb-2" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
        <path stroke-linecap="round" stroke-linejoin="round" d={props.icon} />
      </svg>
      <h3 class="font-semibold text-content text-sm">{props.title}</h3>
      <p class="text-xs text-content-secondary mt-1">{props.desc}</p>
    </Card>
  );
}

function Dashboard() {
  const [progress, setProgress] = createSignal<StudyProgress | null>(null);
  const [stats, setStats] = createSignal<UserStats | null>(null);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    try {
      const [p, s] = await Promise.allSettled([
        studyConfigApi.getProgress(),
        usersApi.getStats(),
      ]);
      if (p.status === 'fulfilled') setProgress(p.value);
      if (s.status === 'fulfilled') setStats(s.value);
    } catch { /* ignore */ }
    setLoading(false);
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <div class="flex items-center justify-between">
        <div>
          <h1 class="text-2xl font-bold text-content">你好, {authStore.user()?.username}</h1>
          <p class="text-content-secondary">今天也要加油哦!</p>
        </div>
        <A href="/learning"><Button size="lg">开始学习</Button></A>
      </div>

      <Show when={!loading()} fallback={<div class="flex justify-center py-8"><Spinner size="lg" /></div>}>
        {/* Today Progress */}
        <Show when={progress()}>
          {(p) => (
            <Card variant="glass" padding="lg">
              <div class="flex items-center justify-between">
                <div>
                  <h2 class="text-lg font-semibold text-content">今日进度</h2>
                  <p class="text-sm text-content-secondary mt-1">已学 {p().studied} / 目标 {p().target}</p>
                  <ProgressBar value={p().studied} max={p().target} color="accent" class="mt-3 w-48" />
                </div>
                <CircularProgress value={p().studied} max={p().target} size={64} strokeWidth={5} />
              </div>
            </Card>
          )}
        </Show>

        {/* Quick Stats */}
        <Show when={stats()}>
          {(s) => (
            <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
              <Card variant="elevated" padding="md">
                <p class="text-2xl font-bold text-accent">{formatNumber(s().totalWordsLearned)}</p>
                <p class="text-xs text-content-secondary">已学单词</p>
              </Card>
              <Card variant="elevated" padding="md">
                <p class="text-2xl font-bold text-warning">{s().streakDays} 天</p>
                <p class="text-xs text-content-secondary">连续学习</p>
              </Card>
              <Card variant="elevated" padding="md">
                <p class="text-2xl font-bold text-success">{formatPercent(s().accuracyRate)}</p>
                <p class="text-xs text-content-secondary">正确率</p>
              </Card>
              <Card variant="elevated" padding="md">
                <p class="text-2xl font-bold text-info">{formatNumber(s().totalRecords)}</p>
                <p class="text-xs text-content-secondary">总记录</p>
              </Card>
            </div>
          )}
        </Show>

        {/* Quick Links */}
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <A href="/learning"><Card variant="outlined" hover padding="md" class="text-center"><p class="text-sm font-medium text-content">四选一学习</p></Card></A>
          <A href="/flashcard"><Card variant="outlined" hover padding="md" class="text-center"><p class="text-sm font-medium text-content">闪记模式</p></Card></A>
          <A href="/vocabulary"><Card variant="outlined" hover padding="md" class="text-center"><p class="text-sm font-medium text-content">词库管理</p></Card></A>
          <A href="/statistics"><Card variant="outlined" hover padding="md" class="text-center"><p class="text-sm font-medium text-content">学习统计</p></Card></A>
        </div>
      </Show>
    </div>
  );
}
