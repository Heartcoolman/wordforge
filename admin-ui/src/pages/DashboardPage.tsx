import { createMemo, createResource, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';
import { StatCard } from '@/components/ui/StatCard';
import { EChart } from '@/components/ui/EChart';
import { Skeleton } from '@/components/ui/Skeleton';
import { WindowPicker } from '@/components/ui/WindowPicker';
import { Panel } from '@/components/ui/Panel';
import { MiniStat } from '@/components/ui/MiniStat';
import { adminApi } from '@/api/admin';
import { formatNumber, formatDuration, formatAccuracy, formatBytes } from '@/utils/formatters';

const DAYS_ALLOWED = [7, 14, 30] as const;
const cssVar = (name: string, fallback: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;

// KPI stagger 入场延迟，避免 4 张卡 fade-in-up 同时触发的"齐刷"感
const STAGGER_FILL = 'backwards' as const;
const STAGGER_DELAYS: Array<{ 'animation-delay'?: string; 'animation-fill-mode': typeof STAGGER_FILL }> = [
  { 'animation-fill-mode': STAGGER_FILL },
  { 'animation-delay': '80ms', 'animation-fill-mode': STAGGER_FILL },
  { 'animation-delay': '160ms', 'animation-fill-mode': STAGGER_FILL },
  { 'animation-delay': '240ms', 'animation-fill-mode': STAGGER_FILL },
];

export default function DashboardPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const days = createMemo(() => {
    const raw = parseInt(searchParams.days as string, 10);
    return DAYS_ALLOWED.includes(raw as typeof DAYS_ALLOWED[number]) ? raw : 7;
  });
  const setDays = (d: number) => setSearchParams({ days: d.toString() });

  // Each resource is independent so a single failure doesn't black-out the page.
  const [stats, { refetch: refetchStats }] = createResource(() => adminApi.getStats());
  const [eng, { refetch: refetchEng }] = createResource(() => adminApi.getEngagement());
  const [overview, { refetch: refetchOverview }] = createResource(days, (d) => adminApi.getStudyOverview(d));
  const [dau] = createResource(days, (d) => adminApi.getDailyActiveUsers(d));
  const [records] = createResource(days, (d) => adminApi.getDailyRecords(d));
  const [health] = createResource(() => adminApi.getHealth());
  const [updateInfo] = createResource(() => adminApi.checkUpdate());

  // 单卡级错误降级：err → 简短错误 + 重试按钮，避免永远转圈的 Skeleton
  const KpiErrorCell = (props: { onRetry: () => void }) => (
    <div class="h-full p-3 rounded-lg border border-error/30 bg-error-light/30 flex flex-col items-start justify-between gap-1.5">
      <p class="text-xs text-error">加载失败</p>
      <button
        type="button"
        class="text-xs px-2 py-0.5 rounded bg-error/10 text-error hover:bg-error/20 transition-colors focus-ring-soft"
        onClick={() => props.onRetry()}
      >
        重试
      </button>
    </div>
  );

  const dauStats = createMemo(() => {
    const data = dau();
    if (!data || data.length === 0) return { avg: 0, peak: 0 };
    return {
      avg: Math.round(data.reduce((a, d) => a + d.count, 0) / data.length),
      peak: Math.max(...data.map((d) => d.count)),
    };
  });

  return (
    <div class="space-y-6">
      <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-2 border-b border-border-hairline">
        <h1 class="text-title text-content">全局概览</h1>
        <WindowPicker value={days()} onChange={setDays} />
      </div>

      {/* KPI 行 — 4 张卡 stagger 80ms 错开；动画放在内层渲染节点，避免 Skeleton/StatCard 切换时闪动 */}
      <div class="grid grid-cols-2 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <div class="h-full">
          <Show
            when={stats()}
            fallback={stats.error ? <KpiErrorCell onRetry={refetchStats} /> : <Skeleton height="100px" />}
          >
            {(s) => (
              <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[0]}>
                <StatCard
                  title="注册用户"
                  value={s().users}
                  format={formatNumber}
                  color="accent"
                  icon="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z"
                  trend={s().trend?.users}
                />
              </div>
            )}
          </Show>
        </div>
        <div class="h-full">
          <Show
            when={eng()}
            fallback={eng.error ? <KpiErrorCell onRetry={refetchEng} /> : <Skeleton height="100px" />}
          >
            {(e) => (
              <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[1]}>
                <StatCard
                  title="今日活跃"
                  value={e().activeToday}
                  format={formatNumber}
                  color="success"
                  icon="M13 10V3L4 14h7v7l9-11h-7z"
                  trend={e().trend?.activeToday}
                />
              </div>
            )}
          </Show>
        </div>
        <div class="h-full">
          <Show
            when={overview()}
            fallback={overview.error ? <KpiErrorCell onRetry={refetchOverview} /> : <Skeleton height="100px" />}
          >
            {(o) => (
              <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[2]}>
                <StatCard
                  title={`${days()}天答题数`}
                  value={o().summary.recordCount}
                  format={formatNumber}
                  color="info"
                  icon="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
                />
              </div>
            )}
          </Show>
        </div>
        <div class="h-full">
          <Show
            when={overview()}
            fallback={overview.error ? <KpiErrorCell onRetry={refetchOverview} /> : <Skeleton height="100px" />}
          >
            {(o) => (
              <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[3]}>
                <StatCard
                  title={`${days()}天正确率`}
                  value={formatAccuracy(o().summary.accuracy)}
                  color="warning"
                  icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                />
              </div>
            )}
          </Show>
        </div>
      </div>

      {/* Panel: 用户活跃趋势 */}
      <Panel
        title="用户活跃趋势"
        aside={
          <Show
            when={dau() && dau()!.length > 0}
            fallback={
              <>
                <Skeleton height="80px" />
                <Skeleton height="80px" />
              </>
            }
          >
            <MiniStat label="平均日活" value={formatNumber(dauStats().avg)} tone="accent" />
            <MiniStat label="峰值日活" value={formatNumber(dauStats().peak)} tone="success" />
          </Show>
        }
      >
        <Show
          when={!dau.error}
          fallback={<Empty title="加载失败" description="无法获取活跃数据" />}
        >
          <Show when={dau()} fallback={<Skeleton height="320px" />}>
            {(data) => (
              <EChart
                option={() => {
                  const accent = cssVar('--accent', '#6366f1');
                  const info = cssVar('--info', '#0ea5e9');
                  return {
                    grid: { left: 40, right: 20, top: 30, bottom: 30 },
                    legend: { data: ['日活跃', '新注册'], top: 0 },
                    tooltip: { trigger: 'axis' },
                    xAxis: { type: 'category', data: data().map((d) => d.date.slice(5)) },
                    yAxis: { type: 'value', minInterval: 1 },
                    series: [
                      {
                        name: '日活跃',
                        type: 'line',
                        data: data().map((d) => d.count),
                        smooth: true,
                        itemStyle: { color: accent },
                        areaStyle: { opacity: 0.18 },
                      },
                      {
                        name: '新注册',
                        type: 'line',
                        data: data().map((d) => d.registered),
                        smooth: true,
                        itemStyle: { color: info },
                        areaStyle: { opacity: 0.12 },
                      },
                    ],
                  };
                }}
              />
            )}
          </Show>
        </Show>
      </Panel>

      {/* Panel: 学习产出 */}
      <Panel
        title="学习产出"
        aside={
          <Show
            when={overview()}
            fallback={
              <>
                <Skeleton height="80px" />
                <Skeleton height="80px" />
              </>
            }
          >
            {(o) => (
              <>
                <MiniStat
                  label={`${days()}天累计学习时长`}
                  value={formatDuration(o().summary.totalDurationSecs)}
                  tone="info"
                />
                <MiniStat
                  label={`${days()}天新增单词`}
                  value={formatNumber(o().summary.newWords)}
                  tone="warning"
                />
              </>
            )}
          </Show>
        }
      >
        <Show
          when={!records.error}
          fallback={<Empty title="加载失败" description="无法获取记录数据" />}
        >
          <Show when={records()} fallback={<Skeleton height="320px" />}>
            {(data) => (
              <EChart
                option={() => {
                  const accent = cssVar('--accent', '#6366f1');
                  const warning = cssVar('--warning', '#eab308');
                  return {
                    grid: { left: 40, right: 50, top: 30, bottom: 30 },
                    legend: { data: ['答题数', '正确率'], top: 0 },
                    tooltip: { trigger: 'axis' },
                    xAxis: { type: 'category', data: data().map((d) => d.date.slice(5)) },
                    yAxis: [
                      { type: 'value', minInterval: 1, name: '答题数' },
                      {
                        type: 'value',
                        name: '正确率',
                        min: 0,
                        max: 100,
                        axisLabel: { formatter: '{value}%' },
                      },
                    ],
                    series: [
                      {
                        name: '答题数',
                        type: 'bar',
                        data: data().map((d) => d.total),
                        itemStyle: { color: accent, borderRadius: [4, 4, 0, 0] },
                      },
                      {
                        name: '正确率',
                        type: 'line',
                        yAxisIndex: 1,
                        smooth: true,
                        data: data().map((d) =>
                          d.total > 0 ? Number(((d.correct / d.total) * 100).toFixed(1)) : 0,
                        ),
                        itemStyle: { color: warning },
                      },
                    ],
                  };
                }}
              />
            )}
          </Show>
        </Show>
      </Panel>

      {/* 系统状态卡 */}
      <Show when={health()}>
        {(h) => (
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-3">系统状态</h2>
            <div class="grid grid-cols-2 sm:grid-cols-2 lg:grid-cols-4 gap-4 text-sm">
              <div>
                <p class="text-content-secondary">状态</p>
                <p class="font-medium flex items-center gap-1.5">
                  <span
                    class={`w-2 h-2 rounded-full ${
                      h().status === 'healthy'
                        ? 'bg-success animate-ring-pulse'
                        : h().status === 'degraded'
                        ? 'bg-warning'
                        : 'bg-error'
                    }`}
                  />
                  <span
                    class={
                      h().status === 'healthy'
                        ? 'text-success'
                        : h().status === 'degraded'
                        ? 'text-warning'
                        : 'text-error'
                    }
                  >
                    {h().status === 'healthy'
                      ? '运行正常'
                      : h().status === 'degraded'
                      ? '性能降级'
                      : '服务异常'}
                  </span>
                </p>
              </div>
              <div>
                <p class="text-content-secondary">数据库大小</p>
                <p class="font-medium text-content tabular-nums">
                  {formatBytes(h().dbSizeBytes)}
                </p>
              </div>
              <div>
                <p class="text-content-secondary">运行时间</p>
                <p class="font-medium text-content tabular-nums">{formatDuration(h().uptimeSecs)}</p>
              </div>
              <div>
                <p class="text-content-secondary">版本</p>
                <div class="flex items-center gap-2 min-w-0">
                  <p class="font-medium text-content truncate max-w-[10ch]" title={h().version}>{h().version}</p>
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

      {/* 各 Resource 已在自身位置展示降级（KPI 卡 / Panel / 系统状态），不再叠加"全失败"Empty */}
    </div>
  );
}
