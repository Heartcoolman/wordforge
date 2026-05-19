import { createMemo, createResource, Show, For } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Skeleton } from '@/components/ui/Skeleton';
import { StatCard } from '@/components/ui/StatCard';
import { EChart } from '@/components/ui/EChart';
import { Empty } from '@/components/ui/Empty';
import { WindowPicker } from '@/components/ui/WindowPicker';
import { adminApi } from '@/api/admin';
import { formatNumber, formatPercent, formatDuration, formatAccuracy } from '@/utils/formatters';

const DAYS_ALLOWED = [7, 14, 30] as const;
const cssVar = (name: string, fallback: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;

const STATE_LABELS: Array<{ key: keyof StateMap; label: string; tone: string }> = [
  { key: 'newCount', label: '未学 (NEW)', tone: '--info' },
  { key: 'learning', label: '学习中 (LEARNING)', tone: '--warning' },
  { key: 'reviewing', label: '复习中 (REVIEWING)', tone: '--accent' },
  { key: 'mastered', label: '已掌握 (MASTERED)', tone: '--success' },
  { key: 'forgotten', label: '已遗忘 (FORGOTTEN)', tone: '--error' },
];

interface StateMap {
  newCount: number;
  learning: number;
  reviewing: number;
  mastered: number;
  forgotten: number;
}

export default function AnalyticsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const days = createMemo(() => {
    const raw = parseInt(searchParams.days as string, 10);
    return DAYS_ALLOWED.includes(raw as typeof DAYS_ALLOWED[number]) ? raw : 7;
  });
  const setDays = (d: number) => setSearchParams({ days: d.toString() });

  const [eng] = createResource(() => adminApi.getEngagement());
  const [learning] = createResource(() => adminApi.getLearningAnalytics());
  const [overview] = createResource(days, (d) => adminApi.getStudyOverview(d));
  const [recordTypes] = createResource(days, (d) => adminApi.getRecordTypes(d));
  const [retention] = createResource(() => adminApi.getRetentionCurve());
  const [states] = createResource(() => adminApi.getWordStateDistribution());

  return (
    <div class="space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-2 border-b border-border-hairline">
        <h1 class="text-title text-content">深度分析</h1>
        <WindowPicker value={days} onChange={setDays} />
      </div>

      {/* KPI 行 (6 张) */}
      <div class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
        <Show when={eng()} fallback={<Skeleton height="100px" />}>
          {(e) => (
            <StatCard
              title="总用户"
              value={formatNumber(e().totalUsers)}
              color="accent"
              icon="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z"
            />
          )}
        </Show>
        <Show when={eng()} fallback={<Skeleton height="100px" />}>
          {(e) => (
            <StatCard
              title="今日活跃"
              value={formatNumber(e().activeToday)}
              color="success"
              icon="M13 10V3L4 14h7v7l9-11h-7z"
              trend={e().trend?.activeToday}
            />
          )}
        </Show>
        <Show when={learning()} fallback={<Skeleton height="100px" />}>
          {(l) => (
            <StatCard
              title="累计答题"
              value={formatNumber(l().totalRecords)}
              color="info"
              icon="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
              trend={l().trend?.totalRecords}
            />
          )}
        </Show>
        <Show when={learning()} fallback={<Skeleton height="100px" />}>
          {(l) => (
            <StatCard
              title="累计正确率"
              value={formatPercent(l().overallAccuracy)}
              color="warning"
              icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
              trend={l().trend?.overallAccuracy}
            />
          )}
        </Show>
        <Show when={overview()} fallback={<Skeleton height="100px" />}>
          {(o) => (
            <StatCard
              title={`${days()}天学习时长`}
              value={formatDuration(o().summary.totalDurationSecs)}
              color="accent"
              icon="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          )}
        </Show>
        <Show when={overview()} fallback={<Skeleton height="100px" />}>
          {(o) => (
            <StatCard
              title={`${days()}天新词`}
              value={formatNumber(o().summary.newWords)}
              color="info"
              icon="M12 6v6m0 0v6m0-6h6m-6 0H6"
            />
          )}
        </Show>
      </div>

      {/* Panel: 学习构成 */}
      <Card variant="elevated">
        <h2 class="text-headline text-content mb-4">学习构成（学习 vs 复习）</h2>
        <Show
          when={!recordTypes.error}
          fallback={<Empty title="加载失败" description="无法获取学习构成数据" />}
        >
          <Show when={recordTypes()} fallback={<Skeleton height="320px" />}>
            {(data) => (
              <EChart
                option={() => {
                  const accent = cssVar('--accent', '#6366f1');
                  const info = cssVar('--info', '#0ea5e9');
                  const warning = cssVar('--warning', '#eab308');
                  return {
                    grid: { left: 50, right: 20, top: 30, bottom: 30 },
                    legend: { data: ['学习', '复习', '未标注'], top: 0 },
                    tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
                    xAxis: { type: 'category', data: data().daily.map((d) => d.date.slice(5)) },
                    yAxis: { type: 'value', minInterval: 1 },
                    series: [
                      {
                        name: '学习',
                        type: 'bar',
                        stack: 'total',
                        data: data().daily.map((d) => d.learning),
                        itemStyle: { color: accent },
                      },
                      {
                        name: '复习',
                        type: 'bar',
                        stack: 'total',
                        data: data().daily.map((d) => d.review),
                        itemStyle: { color: info },
                      },
                      {
                        name: '未标注',
                        type: 'bar',
                        stack: 'total',
                        data: data().daily.map((d) => d.all),
                        itemStyle: { color: warning },
                      },
                    ],
                  };
                }}
              />
            )}
          </Show>
        </Show>
      </Card>

      {/* Panel: 记忆质量 (Retention + State Distribution) */}
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card variant="elevated">
          <div class="flex items-center justify-between mb-4">
            <h2 class="text-headline text-content">记忆遗忘曲线</h2>
            <Show when={retention()?.averageRetention != null}>
              <span class="text-xs text-content-secondary">
                平均留存 {formatAccuracy(retention()!.averageRetention)}
              </span>
            </Show>
          </div>
          <Show
            when={!retention.error}
            fallback={<Empty title="加载失败" description="无法获取保持率数据" />}
          >
            <Show when={retention()} fallback={<Skeleton height="320px" />}>
              {(data) => {
                const empty = data().points.every((p) => p.sampleSize === 0);
                return (
                  <Show when={!empty} fallback={<Empty title="暂无样本" description="过去 31 天内尚无可统计的首学单词" />}>
                    <EChart
                      option={() => {
                        const accent = cssVar('--accent', '#6366f1');
                        return {
                          grid: { left: 50, right: 20, top: 20, bottom: 30 },
                          tooltip: {
                            trigger: 'item',
                            formatter: (p: any) =>
                              `间隔: ${p.data[0]} 天<br/>留存率: ${(p.data[1] * 100).toFixed(1)}%<br/>样本量: ${p.data[2]}`,
                          },
                          xAxis: {
                            type: 'value',
                            name: '间隔（天）',
                            min: 0,
                            max: 32,
                            axisLine: { onZero: false },
                          },
                          yAxis: {
                            type: 'value',
                            name: '留存率',
                            min: 0,
                            max: 1,
                            axisLabel: { formatter: (v: number) => `${(v * 100).toFixed(0)}%` },
                          },
                          series: [
                            {
                              type: 'scatter',
                              symbolSize: (d: number[]) =>
                                Math.max(8, Math.min(28, Math.sqrt(d[2]) * 4)),
                              itemStyle: { color: accent, opacity: 0.8 },
                              data: data()
                                .points.filter((p) => p.retention != null)
                                .map((p) => [p.daysSinceLearn, p.retention, p.sampleSize]),
                            },
                            {
                              type: 'line',
                              smooth: true,
                              showSymbol: false,
                              lineStyle: { color: accent, type: 'dashed', opacity: 0.5 },
                              data: data()
                                .points.filter((p) => p.retention != null)
                                .map((p) => [p.daysSinceLearn, p.retention]),
                            },
                          ],
                        };
                      }}
                    />
                  </Show>
                );
              }}
            </Show>
          </Show>
        </Card>

        <Card variant="elevated">
          <div class="flex items-center justify-between mb-4">
            <h2 class="text-headline text-content">单词状态分布</h2>
            <Show when={states()}>
              <span class="text-xs text-content-secondary">
                跟踪 {formatNumber(states()!.totals.trackedWords)}，已收藏 {formatNumber(states()!.totals.bookmarkedWords)}
              </span>
            </Show>
          </div>
          <Show
            when={!states.error}
            fallback={<Empty title="加载失败" description="无法获取状态分布数据" />}
          >
            <Show when={states()} fallback={<Skeleton height="320px" />}>
              {(data) => {
                const total = data().totals.trackedWords;
                return (
                  <Show when={total > 0} fallback={<Empty title="暂无数据" />}>
                    <div class="grid grid-cols-1 md:grid-cols-5 gap-4">
                      <div class="md:col-span-3">
                        <EChart
                          height="280px"
                          option={() => ({
                            tooltip: { trigger: 'item', formatter: '{b}: {c} ({d}%)' },
                            series: [
                              {
                                type: 'pie',
                                radius: ['55%', '78%'],
                                avoidLabelOverlap: true,
                                label: { show: false },
                                itemStyle: { borderRadius: 4, borderColor: '#fff', borderWidth: 2 },
                                data: STATE_LABELS.map((s) => ({
                                  value: data().states[s.key],
                                  name: s.label,
                                  itemStyle: { color: cssVar(s.tone, '#6366f1') },
                                })),
                              },
                            ],
                          })}
                        />
                      </div>
                      <div class="md:col-span-2 flex flex-col justify-center gap-2 text-sm">
                        <For each={STATE_LABELS}>
                          {(s) => {
                            const v = data().states[s.key];
                            const pct = total > 0 ? (v / total) * 100 : 0;
                            return (
                              <div class="flex items-center justify-between gap-2">
                                <span class="flex items-center gap-2 text-content-secondary">
                                  <span
                                    class="w-2.5 h-2.5 rounded-full inline-block"
                                    style={{ 'background-color': cssVar(s.tone, '#6366f1') }}
                                  />
                                  {s.label}
                                </span>
                                <span class="font-medium text-content tabular-nums">
                                  {formatNumber(v)} · {pct.toFixed(1)}%
                                </span>
                              </div>
                            );
                          }}
                        </For>
                      </div>
                    </div>
                  </Show>
                );
              }}
            </Show>
          </Show>
        </Card>
      </div>
    </div>
  );
}
