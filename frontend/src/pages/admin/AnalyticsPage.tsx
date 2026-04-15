import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Skeleton } from '@/components/ui/Skeleton';
import { StatCard } from '@/components/ui/StatCard';
import { EChart } from '@/components/ui/EChart';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { formatNumber, formatPercent } from '@/utils/formatters';
import type { EngagementAnalytics, LearningAnalytics, DailyRecordsEntry } from '@/types/admin';

export default function AnalyticsPage() {
  const [engagement, setEngagement] = createSignal<EngagementAnalytics | null>(null);
  const [learning, setLearning] = createSignal<LearningAnalytics | null>(null);
  const [dailyRecords, setDailyRecords] = createSignal<DailyRecordsEntry[] | null>(null);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    const [e, l, dr] = await Promise.allSettled([
      adminApi.getEngagement(),
      adminApi.getLearningAnalytics(),
      adminApi.getDailyRecords(7),
    ]);
    if (e.status === 'fulfilled') setEngagement(e.value);
    if (l.status === 'fulfilled') setLearning(l.value);
    if (dr.status === 'fulfilled') setDailyRecords(dr.value);
    if (e.status === 'rejected' && l.status === 'fulfilled') {
      uiStore.toast.warning('活跃度数据加载失败', e.reason instanceof Error ? e.reason.message : '');
    } else if (l.status === 'rejected' && e.status === 'fulfilled') {
      uiStore.toast.warning('学习数据加载失败', l.reason instanceof Error ? l.reason.message : '');
    }
    setLoading(false);
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={engagement()}>
          {(e) => (
            <>
              <h2 class="text-lg font-semibold text-content">用户活跃度</h2>
              <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                <StatCard title="总用户" value={formatNumber(e().totalUsers)} color="accent"
                  icon="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" />
                <StatCard title="今日活跃" value={formatNumber(e().activeToday)} color="success"
                  icon="M13 10V3L4 14h7v7l9-11h-7z"
                  trend={e().trend?.activeToday} />
                <StatCard title="日活跃率" value={formatPercent(e().retentionRate)} color="info"
                  icon="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
              </div>
            </>
          )}
        </Show>

        <Show when={learning()}>
          {(l) => (
            <>
              <h2 class="text-lg font-semibold text-content">学习数据</h2>
              <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
                <StatCard title="总单词" value={formatNumber(l().totalWords)} color="accent"
                  icon="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
                <StatCard title="总记录" value={formatNumber(l().totalRecords)} color="info"
                  icon="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
                  trend={l().trend?.totalRecords} />
                <StatCard title="正确数" value={formatNumber(l().totalCorrect)} color="success"
                  icon="M5 13l4 4L19 7" />
                <StatCard title="总正确率" value={formatPercent(l().overallAccuracy)} color="warning"
                  icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                  trend={l().trend?.overallAccuracy} />
              </div>
            </>
          )}
        </Show>

        <Card variant="elevated">
          <h2 class="text-lg font-semibold text-content mb-3">每日学习记录</h2>
          <Show when={dailyRecords()} fallback={<Skeleton height="320px" />}>
            {(data) => {
              const chartOption = () => {
                const success = getComputedStyle(document.documentElement).getPropertyValue('--success').trim() || '#22c55e';
                const error = getComputedStyle(document.documentElement).getPropertyValue('--error').trim() || '#ef4444';
                return {
                  grid: { left: 50, right: 20, top: 30, bottom: 30 },
                  legend: { data: ['正确', '错误'] },
                  tooltip: {
                    trigger: 'axis' as const,
                    formatter: (params: any) => {
                      const d = params[0];
                      const correct = d.data as number;
                      const incorrect = params[1]?.data as number ?? 0;
                      const total = correct + incorrect;
                      const acc = total > 0 ? (correct / total * 100).toFixed(1) : '0.0';
                      return `${d.axisValue}<br/>正确: ${correct}<br/>错误: ${incorrect}<br/>总计: ${total}<br/>正确率: ${acc}%`;
                    },
                  },
                  xAxis: { type: 'category' as const, data: data().map(d => d.date.slice(5)) },
                  yAxis: { type: 'value' as const, minInterval: 1 },
                  series: [
                    { name: '正确', type: 'bar' as const, data: data().map(d => d.correct), itemStyle: { color: success } },
                    { name: '错误', type: 'bar' as const, data: data().map(d => d.total - d.correct), itemStyle: { color: error } },
                  ],
                };
              };
              return <EChart option={chartOption} />;
            }}
          </Show>
        </Card>
      </Show>
    </div>
  );
}
