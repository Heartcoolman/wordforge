import { createSignal, Show, For, onMount, onCleanup } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { ProgressBar } from '@/components/ui/Progress';
import { Empty } from '@/components/ui/Empty';
import { usersApi } from '@/api/users';
import { recordsApi } from '@/api/records';
import { amasApi } from '@/api/amas';
import { wordStatesApi } from '@/api/wordStates';
import type { UserStats } from '@/types/user';
import type { EnhancedStatistics } from '@/types/record';
import type { AmasUserState, AmasIntervention, LearningCurvePoint, ColdStartPhase } from '@/types/amas';
import type { WordStateOverview } from '@/types/wordState';
import { formatPercent, formatNumber } from '@/utils/formatters';
import { DAILY_CHART_DAYS, ACCURACY_HIGH_THRESHOLD, ACCURACY_MID_THRESHOLD } from '@/lib/constants';

export default function StatisticsPage() {
  const [stats, setStats] = createSignal<UserStats | null>(null);
  const [enhanced, setEnhanced] = createSignal<EnhancedStatistics | null>(null);
  const [amasState, setAmasState] = createSignal<AmasUserState | null>(null);
  const [wordOverview, setWordOverview] = createSignal<WordStateOverview | null>(null);
  const [learningCurve, setLearningCurve] = createSignal<LearningCurvePoint[]>([]);
  const [interventions, setInterventions] = createSignal<AmasIntervention[]>([]);
  const [phase, setPhase] = createSignal<ColdStartPhase | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [sseConnected, setSseConnected] = createSignal(false);
  let unsubscribeStateEvents: (() => void) | undefined;

  onMount(async () => {
    const [s, e, a, w, lc, iv, ph] = await Promise.allSettled([
      usersApi.getStats(),
      recordsApi.enhancedStatistics(),
      amasApi.getState(),
      wordStatesApi.getOverview(),
      amasApi.getLearningCurve(),
      amasApi.getIntervention(),
      amasApi.getPhase(),
    ]);
    if (s.status === 'fulfilled') setStats(s.value);
    if (e.status === 'fulfilled') setEnhanced(e.value);
    if (a.status === 'fulfilled') setAmasState(a.value);
    if (w.status === 'fulfilled') setWordOverview(w.value);
    if (lc.status === 'fulfilled') setLearningCurve(lc.value.curve);
    if (iv.status === 'fulfilled') setInterventions(iv.value.interventions);
    if (ph.status === 'fulfilled') setPhase(ph.value.phase);
    setLoading(false);

    unsubscribeStateEvents = amasApi.subscribeStateEvents((event) => {
      setSseConnected(true);
      setAmasState((prev) => {
        if (!prev) return { ...event, createdAt: new Date().toISOString() };
        return {
          ...prev,
          ...event,
        };
      });
    });
  });

  onCleanup(() => {
    unsubscribeStateEvents?.();
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">学习统计</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={stats() || enhanced() || amasState() || wordOverview()} fallback={
          <Empty title="暂无统计数据" description="开始学习后，你的学习统计将显示在这里" />
        }>
        {/* Overview Cards */}
        <Show when={stats()}>
          {(s) => (
            <div class="grid grid-cols-2 md:grid-cols-5 gap-4">
              <StatCard label="学习单词" value={formatNumber(s().totalWordsLearned)} icon="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" color="accent" />
              <StatCard label="学习会话数" value={formatNumber(s().totalSessions)} icon="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" color="info" />
              <StatCard label="总记录数" value={formatNumber(s().totalRecords)} icon="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" color="warning" />
              <StatCard label="连续天数" value={`${s().streakDays} 天`} icon="M17.657 18.657A8 8 0 016.343 7.343S7 9 9 10c0-2 .5-5 2.986-7C14 5 16.09 5.777 17.656 7.343A7.975 7.975 0 0120 13a7.975 7.975 0 01-2.343 5.657z" color="error" />
              <StatCard label="正确率" value={formatPercent(s().accuracyRate)} icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" color="success" />
            </div>
          )}
        </Show>

        {/* Word State Distribution */}
        <Show when={wordOverview()}>
          {(wo) => {
            const total = () => wo().newCount + wo().learning + wo().reviewing + wo().mastered + wo().forgotten;
            return (
              <Card variant="elevated">
                <h2 class="text-lg font-semibold text-content mb-4">单词状态分布</h2>
                <div class="grid grid-cols-2 md:grid-cols-5 gap-4 mb-4">
                  <div class="text-center">
                    <p class="text-xl font-bold text-content-tertiary">{wo().newCount}</p>
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
                  <div class="text-center">
                    <p class="text-xl font-bold text-error">{wo().forgotten}</p>
                    <p class="text-xs text-content-secondary">已遗忘</p>
                  </div>
                </div>
                <Show when={total() > 0}>
                  <div class="flex rounded-full overflow-hidden h-3">
                    <div class="bg-surface-tertiary" style={{ width: `${(wo().newCount / total()) * 100}%` }} />
                    <div class="bg-info" style={{ width: `${(wo().learning / total()) * 100}%` }} />
                    <div class="bg-warning" style={{ width: `${(wo().reviewing / total()) * 100}%` }} />
                    <div class="bg-success" style={{ width: `${(wo().mastered / total()) * 100}%` }} />
                    <div class="bg-error" style={{ width: `${(wo().forgotten / total()) * 100}%` }} />
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
              <div class="flex items-center justify-between mb-4">
                <h2 class="text-lg font-semibold text-content">认知状态</h2>
                <span class={`inline-flex items-center gap-1.5 text-xs ${sseConnected() ? 'text-success' : 'text-content-tertiary'}`}>
                  <span class={`w-2 h-2 rounded-full ${sseConnected() ? 'bg-success animate-pulse' : 'bg-content-tertiary'}`} />
                  {sseConnected() ? '实时更新中' : '等待连接'}
                </span>
              </div>
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
              <For each={enhanced()!.daily.slice(-DAILY_CHART_DAYS)}>
                {(day) => (
                  <div class="flex items-center gap-3 text-sm">
                    <span class="w-20 text-content-secondary text-xs">{day.date}</span>
                    <div class="flex-1"><ProgressBar value={day.total} max={Math.max(...enhanced()!.daily.map((d) => d.total), 1)} size="sm" color={day.accuracy > ACCURACY_HIGH_THRESHOLD ? 'success' : day.accuracy > ACCURACY_MID_THRESHOLD ? 'warning' : 'error'} /></div>
                    <span class="w-12 text-right text-xs text-content-tertiary">{day.total} 题</span>
                    <span class="w-12 text-right text-xs text-content-tertiary">{(day.accuracy * 100).toFixed(0)}%</span>
                  </div>
                )}
              </For>
            </div>
          </Card>
        </Show>

        {/* Learning Phase */}
        <Show when={phase()}>
          {(p) => {
            const phaseLabel: Record<ColdStartPhase, string> = { Classify: '分类阶段', Explore: '探索阶段', Exploit: '优化阶段' };
            const phaseDesc: Record<ColdStartPhase, string> = {
              Classify: '系统正在评估你的基础水平',
              Explore: '系统正在探索适合你的学习策略',
              Exploit: '系统已找到最佳策略，持续优化中',
            };
            const phaseColor: Record<ColdStartPhase, string> = { Classify: 'warning', Explore: 'info', Exploit: 'success' };
            return (
              <Card variant="elevated">
                <div class="flex items-center justify-between">
                  <h2 class="text-lg font-semibold text-content">学习阶段</h2>
                  <Badge variant={phaseColor[p()] as 'success'}>{phaseLabel[p()]}</Badge>
                </div>
                <p class="text-sm text-content-secondary mt-2">{phaseDesc[p()]}</p>
              </Card>
            );
          }}
        </Show>

        {/* Intervention Suggestions */}
        <Show when={interventions().length > 0}>
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-3">学习建议</h2>
            <div class="space-y-2">
              <For each={interventions()}>
                {(item) => {
                  const severityMap: Record<string, string> = { warning: 'bg-warning-light text-warning', info: 'bg-info-light text-info', success: 'bg-success-light text-success' };
                  const iconMap: Record<string, string> = {
                    rest: 'M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z',
                    encouragement: 'M14.828 14.828a4 4 0 01-5.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
                    focus: 'M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z',
                    continue: 'M13 10V3L4 14h7v7l9-11h-7z',
                  };
                  return (
                    <div class={`flex items-start gap-3 p-3 rounded-lg ${severityMap[item.severity] ?? 'bg-surface-secondary text-content'}`}>
                      <svg class="w-5 h-5 flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d={iconMap[item.type] ?? iconMap.continue} />
                      </svg>
                      <p class="text-sm">{item.message}</p>
                    </div>
                  );
                }}
              </For>
            </div>
          </Card>
        </Show>

        {/* Learning Curve */}
        <Show when={learningCurve().length > 0}>
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-4">学习曲线</h2>
            <div class="space-y-2">
              <For each={learningCurve().slice(-DAILY_CHART_DAYS)}>
                {(pt) => (
                  <div class="flex items-center gap-3 text-sm">
                    <span class="w-20 text-content-secondary text-xs">{pt.date}</span>
                    <div class="flex-1"><ProgressBar value={pt.accuracy * 100} max={100} size="sm" color={pt.accuracy > ACCURACY_HIGH_THRESHOLD ? 'success' : pt.accuracy > ACCURACY_MID_THRESHOLD ? 'warning' : 'error'} /></div>
                    <span class="w-14 text-right text-xs text-content-tertiary">{pt.correct}/{pt.total}</span>
                    <span class="w-12 text-right text-xs text-content-tertiary">{(pt.accuracy * 100).toFixed(0)}%</span>
                  </div>
                )}
              </For>
            </div>
          </Card>
        </Show>
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
