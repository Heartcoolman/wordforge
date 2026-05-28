import { createMemo, createResource, For, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';
import { StatCard } from '@/components/ui/StatCard';
import { EChart } from '@/components/ui/EChart';
import { Skeleton } from '@/components/ui/Skeleton';
import { WindowPicker } from '@/components/ui/WindowPicker';
import { Panel } from '@/components/ui/Panel';
import { MiniStat } from '@/components/ui/MiniStat';
import { HeroCard } from '@/components/ui/HeroCard';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
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
  // 近期告警：取最近 20 条 AMAS 监控事件,只筛 error/warning 级别(eventType 含 error|warn|fail|denied)
  const [monitoring] = createResource(() => amasApi.getMonitoring(20).catch(() => []));

  // sparkline series:dau.registered 是每日新增注册,dau.count 是每日活跃
  const registeredSpark = createMemo(() => dau()?.map((d) => d.registered) ?? []);
  const activeSpark = createMemo(() => dau()?.map((d) => d.count) ?? []);
  const recordsSpark = createMemo(() => records()?.map((d) => d.total) ?? []);
  const accuracySpark = createMemo(
    () => records()?.map((d) => (d.total > 0 ? (d.correct / d.total) * 100 : 0)) ?? [],
  );

  // 近期告警筛选 + 视觉级别
  const alerts = createMemo(() => {
    const events = monitoring() ?? [];
    return events
      .filter((e) => /error|warn|fail|denied|critical/i.test(e.eventType))
      .slice(0, 6);
  });
  function alertSeverity(eventType: string): { tone: 'error' | 'warning'; label: string } {
    if (/error|fail|critical|denied/i.test(eventType)) return { tone: 'error', label: '错误' };
    return { tone: 'warning', label: '警告' };
  }

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

  const healthEyebrow = createMemo(() => {
    const h = health();
    if (!h) return { text: '检查中…', variant: 'info' as const };
    if (h.status === 'healthy') return { text: '运行正常', variant: 'success' as const };
    if (h.status === 'degraded') return { text: '性能降级', variant: 'warning' as const };
    return { text: '服务异常', variant: 'error' as const };
  });

  return (
    <div class="space-y-6">
      {/* Hero — 全局健康状态 + 4 KPI count-up（来自 stats / eng / overview，sparkline 在 KPI 行另外展示） */}
      <HeroCard
        eyebrow={healthEyebrow().text}
        eyebrowVariant={healthEyebrow().variant}
        title="全局概览"
        desc="今日学习产出、用户活跃、AMAS 健康状态一屏看完。窗口可切 7 / 14 / 30 天。"
        cta={<WindowPicker value={days()} onChange={setDays} />}
        meta={[
          { value: stats()?.users ?? 0, label: '注册用户', format: formatNumber },
          { value: eng()?.activeToday ?? 0, label: '今日活跃', format: formatNumber },
          { value: overview()?.summary.recordCount ?? 0, label: `${days()}天答题`, format: formatNumber },
          { value: overview()?.summary.accuracy != null ? formatAccuracy(overview()!.summary.accuracy) : '—', label: `${days()}天正确率` },
        ]}
      />

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
                  spark={registeredSpark()}
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
                  spark={activeSpark()}
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
                  spark={recordsSpark()}
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
                  spark={accuracySpark()}
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

      {/* 近期告警：从 AMAS monitoring 取最近 error/warning 级别事件,最多 6 条 */}
      <Card variant="elevated">
        <div class="flex items-center justify-between mb-3">
          <div class="flex items-center gap-2">
            <h2 class="text-lg font-semibold text-content">近期告警</h2>
            <Show when={alerts().length > 0}>
              <span class="inline-flex items-center px-2 py-0.5 rounded-pill text-[11px] font-medium bg-warning-light text-warning-strong tabular-nums">
                {alerts().length}
              </span>
            </Show>
          </div>
          <a
            href="/admin/monitoring"
            class="text-[12.5px] text-content-tertiary hover:text-accent inline-flex items-center gap-0.5"
          >
            查看全部 →
          </a>
        </div>
        <Show
          when={!monitoring.loading}
          fallback={<Skeleton height="80px" />}
        >
          <Show
            when={alerts().length > 0}
            fallback={
              <div class="flex items-center gap-2 py-3 text-[13px] text-content-tertiary">
                <span class="inline-block size-1.5 rounded-full bg-success animate-ring-pulse" aria-hidden="true" />
                近 20 条监控事件未发现告警,系统运行平稳。
              </div>
            }
          >
            <ul class="divide-y divide-border-hairline">
              <For each={alerts()}>
                {(ev) => {
                  const sev = alertSeverity(ev.eventType);
                  return (
                    <li class="flex items-start gap-3 py-2.5 text-[13px]">
                      <span
                        class={`mt-1 inline-block size-2 rounded-full shrink-0 ${
                          sev.tone === 'error' ? 'bg-error' : 'bg-warning'
                        }`}
                        aria-hidden="true"
                      />
                      <div class="min-w-0 flex-1">
                        <div class="flex items-center gap-2">
                          <span class={`font-mono text-[11.5px] uppercase tracking-wide ${
                            sev.tone === 'error' ? 'text-error-strong' : 'text-warning-strong'
                          }`}>{sev.label}</span>
                          <span class="font-mono text-content text-[12.5px] truncate">{ev.eventType}</span>
                        </div>
                        <Show when={Object.keys(ev.data).length > 0}>
                          <p class="text-content-tertiary text-[11.5px] truncate font-mono">
                            {JSON.stringify(ev.data).slice(0, 120)}
                          </p>
                        </Show>
                      </div>
                      <time class="text-content-tertiary text-[11.5px] tabular-nums shrink-0">
                        {new Date(ev.timestamp).toLocaleString('zh-CN', { hour12: false })}
                      </time>
                    </li>
                  );
                }}
              </For>
            </ul>
          </Show>
        </Show>
      </Card>

      {/* 各 Resource 已在自身位置展示降级（KPI 卡 / Panel / 系统状态），不再叠加"全失败"Empty */}
    </div>
  );
}
