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
import { formatNumber, formatDuration, formatAccuracy } from '@/utils/formatters';

const DAYS_ALLOWED = [7, 14, 30] as const;
const cssVar = (name: string, fallback: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;

export default function AdminDashboard() {
  const [searchParams, setSearchParams] = useSearchParams();
  const days = createMemo(() => {
    const raw = parseInt(searchParams.days as string, 10);
    return DAYS_ALLOWED.includes(raw as typeof DAYS_ALLOWED[number]) ? raw : 7;
  });
  const setDays = (d: number) => setSearchParams({ days: d.toString() });

  // Each resource is independent so a single failure doesn't black-out the page.
  const [stats] = createResource(() => adminApi.getStats());
  const [eng] = createResource(() => adminApi.getEngagement());
  const [overview] = createResource(days, (d) => adminApi.getStudyOverview(d));
  const [dau] = createResource(days, (d) => adminApi.getDailyActiveUsers(d));
  const [records] = createResource(days, (d) => adminApi.getDailyRecords(d));
  const [health] = createResource(() => adminApi.getHealth());
  const [updateInfo] = createResource(() => adminApi.checkUpdate());

  const dauStats = createMemo(() => {
    const data = dau();
    if (!data || data.length === 0) return { avg: 0, peak: 0 };
    return {
      avg: Math.round(data.reduce((a, d) => a + d.count, 0) / data.length),
      peak: Math.max(...data.map((d) => d.count)),
    };
  });

  return (
    <div class="space-y-6 animate-fade-in-up">
      <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-2 border-b border-border">
        <h1 class="text-xl font-bold text-content">全局概览</h1>
        <WindowPicker value={days} onChange={setDays} />
      </div>

      {/* KPI 行 */}
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Show when={stats()} fallback={<Skeleton height="100px" />}>
          {(s) => (
            <StatCard
              title="注册用户"
              value={formatNumber(s().users)}
              color="accent"
              icon="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z"
              trend={s().trend?.users}
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
        <Show when={overview()} fallback={<Skeleton height="100px" />}>
          {(o) => (
            <StatCard
              title={`${days()}天答题数`}
              value={formatNumber(o().summary.recordCount)}
              color="info"
              icon="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
            />
          )}
        </Show>
        <Show when={overview()} fallback={<Skeleton height="100px" />}>
          {(o) => (
            <StatCard
              title={`${days()}天正确率`}
              value={formatAccuracy(o().summary.accuracy)}
              color="warning"
              icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          )}
        </Show>
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
            <div class="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div>
                <p class="text-content-secondary">状态</p>
                <p class="font-medium flex items-center gap-1.5">
                  <span
                    class={`w-2 h-2 rounded-full ${
                      h().status === 'healthy'
                        ? 'bg-success'
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
                <p class="font-medium text-content">
                  {(h().dbSizeBytes / 1024 / 1024).toFixed(2)} MB
                </p>
              </div>
              <div>
                <p class="text-content-secondary">运行时间</p>
                <p class="font-medium text-content">{formatDuration(h().uptimeSecs)}</p>
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

      <Show
        when={
          stats.error && eng.error && overview.error && dau.error && records.error && health.error
        }
      >
        <Empty title="加载失败" description="无法获取仪表盘数据，请稍后重试" />
      </Show>
    </div>
  );
}
