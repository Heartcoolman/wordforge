import { createMemo, createResource, Show, For } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { HeroCard } from '@/components/ui/HeroCard';
import { Skeleton } from '@/components/ui/Skeleton';
import { StatCard } from '@/components/ui/StatCard';
import { EChart } from '@/components/ui/EChart';
import { Empty } from '@/components/ui/Empty';
import { Button } from '@/components/ui/Button';
import { WindowPicker } from '@/components/ui/WindowPicker';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
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
  // m022:新增 3 端点
  const [hourly] = createResource(days, (d) => adminApi.analyticsHourly(d));
  const [wbRank] = createResource(days, (d) => adminApi.analyticsWordbookRank({ days: d, limit: 10 }));
  const [cohort] = createResource(() => adminApi.analyticsRetentionCohort({ cohort: 'weekly', maxDays: 28 }));

  // 中文星期 label,用于 hourly heatmap y 轴
  const DOW_LABEL = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

  // 导出 CSV:把当前已加载的 retention + state + record types 拼成一个表
  function exportCsv() {
    const rows: string[][] = [['section', 'metric', 'value']];
    const e = eng();
    const o = overview();
    if (e) {
      rows.push(['engagement', 'totalUsers', String(e.totalUsers)]);
      rows.push(['engagement', 'activeToday', String(e.activeToday)]);
      rows.push(['engagement', 'retentionRate', formatPercent(e.retentionRate)]);
    }
    if (o) {
      rows.push(['overview', `${days()}d_records`, String(o.summary.recordCount)]);
      rows.push(['overview', `${days()}d_accuracy`, formatAccuracy(o.summary.accuracy)]);
      rows.push(['overview', `${days()}d_duration_secs`, String(o.summary.totalDurationSecs)]);
      rows.push(['overview', `${days()}d_new_words`, String(o.summary.newWords)]);
    }
    const r = retention();
    if (r) {
      r.points.forEach((p) => {
        rows.push(['retention', `${p.daysSinceLearn}d_since_learn`, `${p.retention ?? 'null'}|n=${p.sampleSize}`]);
      });
    }
    const s = states();
    if (s) {
      Object.entries(s.states).forEach(([k, v]) => {
        rows.push(['state_distribution', k, String(v)]);
      });
    }
    if (rows.length === 1) {
      uiStore.toast.warning('没有可导出的数据,请等待图表全部加载');
      return;
    }
    const csv = rows
      .map((r) => r.map((cell) => (cell.includes(',') || cell.includes('"') ? `"${cell.replace(/"/g, '""')}"` : cell)).join(','))
      .join('\n');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `wordforge-analytics-${new Date().toISOString().slice(0, 10)}.csv`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    uiStore.toast.success(`已导出 ${rows.length - 1} 行 CSV`);
  }

  return (
    <div class="space-y-6">
      <HeroCard
        eyebrow="留存 + 漏斗 + 时段"
        eyebrowVariant="info"
        title="数据分析"
        desc={`过去 ${days()} 天的学习深度数据。留存矩阵、答题分布、时段热图、词库使用 top。可导出 CSV。`}
        cta={
          <div class="flex flex-wrap items-center gap-2">
            <WindowPicker value={days()} onChange={setDays} />
            <Button size="sm" variant="ghost" onClick={exportCsv}>导出 CSV</Button>
          </div>
        }
      />

      {/* KPI 行 (6 张) — 中等宽度先 2 行 3 列，lg/xl 展开 6 卡，避免 1024-1279 区间挤压 */}
      <div class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 xl:grid-cols-6 gap-3">
        <Show when={eng()} fallback={<Skeleton height="84px" />}>
          {(e) => (
            <StatCard
              size="sm"
              title="总用户"
              value={formatNumber(e().totalUsers)}
              color="accent"
              icon="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z"
            />
          )}
        </Show>
        <Show when={eng()} fallback={<Skeleton height="84px" />}>
          {(e) => (
            <StatCard
              size="sm"
              title="今日活跃"
              value={formatNumber(e().activeToday)}
              color="success"
              icon="M13 10V3L4 14h7v7l9-11h-7z"
              trend={e().trend?.activeToday}
            />
          )}
        </Show>
        <Show when={learning()} fallback={<Skeleton height="84px" />}>
          {(l) => (
            <StatCard
              size="sm"
              title="累计答题"
              value={formatNumber(l().totalRecords)}
              color="info"
              icon="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"
              trend={l().trend?.totalRecords}
            />
          )}
        </Show>
        <Show when={learning()} fallback={<Skeleton height="84px" />}>
          {(l) => (
            <StatCard
              size="sm"
              title="累计正确率"
              value={formatPercent(l().overallAccuracy)}
              color="warning"
              icon="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
              trend={l().trend?.overallAccuracy}
            />
          )}
        </Show>
        <Show when={overview()} fallback={<Skeleton height="84px" />}>
          {(o) => (
            <StatCard
              size="sm"
              title={`${days()}天学习时长`}
              value={formatDuration(o().summary.totalDurationSecs)}
              color="accent"
              icon="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          )}
        </Show>
        <Show when={overview()} fallback={<Skeleton height="84px" />}>
          {(o) => (
            <StatCard
              size="sm"
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
                            // echarts 回调可能传单值或数组（multi-axis tooltip）；scatter 单点，取首项后断言为元组
                            formatter: (p) => {
                              const param = Array.isArray(p) ? p[0] : p;
                              const d = (param as unknown as { data: [number, number, number] }).data;
                              return `间隔: ${d[0]} 天<br/>留存率: ${(d[1] * 100).toFixed(1)}%<br/>样本量: ${d[2]}`;
                            },
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
                                itemStyle: { borderRadius: 4, borderColor: cssVar('--surface', '#fff'), borderWidth: 2 },
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

      {/* m022:时段热图 + 词库 top10 (来自 /admin/analytics/hourly + wordbook-rank) */}
      <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <Card variant="elevated">
          <div class="flex items-center justify-between mb-3">
            <h2 class="text-headline text-content">时段活跃热图</h2>
            <Show when={hourly()}>
              <span class="text-xs text-content-secondary">总样本 {formatNumber(hourly()!.total)}</span>
            </Show>
          </div>
          <Show
            when={!hourly.error}
            fallback={<Empty title="加载失败" description="无法获取 hourly buckets" />}
          >
            <Show when={hourly()} fallback={<Skeleton height="320px" />}>
              {(data) => {
                // matrix 7×24 → echarts heatmap 需要 [hour, dow, value] 三元组
                const points: Array<[number, number, number]> = [];
                let maxV = 0;
                data().matrix.forEach((row, dow) => {
                  row.forEach((v, hour) => {
                    points.push([hour, dow, v]);
                    if (v > maxV) maxV = v;
                  });
                });
                return (
                  <Show when={maxV > 0} fallback={<Empty title="过去 N 天无答题样本" />}>
                    <EChart
                      height="320px"
                      option={() => {
                        const accent = cssVar('--accent', '#6366f1');
                        const surface = cssVar('--surface-tertiary', '#f3f4f6');
                        return {
                          tooltip: {
                            position: 'top',
                            formatter: (p: unknown) => {
                              const param = Array.isArray(p) ? p[0] : p;
                              const [hr, dw, val] = (param as { data: [number, number, number] }).data;
                              return `${DOW_LABEL[dw]} ${String(hr).padStart(2, '0')}:00<br/>答题 ${val}`;
                            },
                          },
                          grid: { left: 50, right: 20, top: 20, bottom: 50 },
                          xAxis: {
                            type: 'category',
                            data: Array.from({ length: 24 }, (_, i) => String(i).padStart(2, '0')),
                            splitArea: { show: true },
                          },
                          yAxis: {
                            type: 'category',
                            data: DOW_LABEL,
                            splitArea: { show: true },
                          },
                          visualMap: {
                            min: 0,
                            max: maxV,
                            calculable: true,
                            orient: 'horizontal',
                            left: 'center',
                            bottom: 5,
                            inRange: { color: [surface, accent] },
                            textStyle: { fontSize: 10 },
                          },
                          series: [
                            {
                              type: 'heatmap',
                              data: points,
                              label: { show: false },
                              emphasis: { itemStyle: { shadowBlur: 10 } },
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
          <div class="flex items-center justify-between mb-3">
            <h2 class="text-headline text-content">词库使用 Top 10</h2>
            <Show when={wbRank()}>
              <span class="text-xs text-content-secondary">过去 {wbRank()!.days} 天</span>
            </Show>
          </div>
          <Show
            when={!wbRank.error}
            fallback={<Empty title="加载失败" description="无法获取词库排名数据" />}
          >
            <Show when={wbRank()} fallback={<Skeleton height="320px" />}>
              {(data) => (
                <Show when={data().rows.length > 0} fallback={<Empty title="过去 N 天无词库使用记录" />}>
                  <EChart
                    height="320px"
                    option={() => {
                      const accent = cssVar('--accent', '#6366f1');
                      const info = cssVar('--info', '#0ea5e9');
                      // 横向柱图:y category = 词书名,x value = 答题数,叠加正确率提示
                      const reversed = [...data().rows].reverse(); // echarts 横向柱底部=第一项,反转使最大在顶
                      return {
                        grid: { left: 130, right: 30, top: 20, bottom: 30 },
                        tooltip: {
                          trigger: 'axis',
                          axisPointer: { type: 'shadow' },
                          formatter: (p: unknown) => {
                            const arr = Array.isArray(p) ? p : [p];
                            const first = arr[0] as { dataIndex: number; value: number };
                            const row = reversed[first.dataIndex];
                            const acc = row.accuracy != null ? `${(row.accuracy * 100).toFixed(1)}%` : '—';
                            return `<b>${row.name}</b><br/>答题 ${row.recordCount} · 正确 ${row.correctCount}<br/>学习者 ${row.learnerCount} · 正确率 ${acc}`;
                          },
                        },
                        xAxis: { type: 'value', name: '答题数', axisLabel: { fontSize: 11 } },
                        yAxis: {
                          type: 'category',
                          data: reversed.map((r) => r.name.length > 14 ? `${r.name.slice(0, 12)}…` : r.name),
                          axisLabel: { fontSize: 11 },
                        },
                        series: [
                          {
                            type: 'bar',
                            barWidth: '60%',
                            itemStyle: {
                              color: (params: { dataIndex: number }) => {
                                const row = reversed[params.dataIndex];
                                // 学习者多用 accent,少用 info
                                return row.learnerCount > 10 ? accent : info;
                              },
                              borderRadius: [0, 4, 4, 0],
                            },
                            data: reversed.map((r) => r.recordCount),
                          },
                        ],
                      };
                    }}
                  />
                </Show>
              )}
            </Show>
          </Show>
        </Card>
      </div>

      {/* m022:cohort 留存曲线(按周聚合) */}
      <Card variant="elevated">
        <div class="flex items-center justify-between mb-3">
          <h2 class="text-headline text-content">用户留存(按注册周分组)</h2>
          <Show when={cohort()}>
            <span class="text-xs text-content-secondary">最近 {cohort()!.maxDays} 天 · 按 {cohort()!.cohortUnit} 聚合</span>
          </Show>
        </div>
        <Show
          when={!cohort.error}
          fallback={<Empty title="加载失败" description="无法获取 cohort 数据" />}
        >
          <Show when={cohort()} fallback={<Skeleton height="320px" />}>
            {(data) => {
              // 按 cohortStart 分组,每个 cohort 一条线;x = daysSince, y = retainedUsers
              const cohorts = new Map<string, Array<{ d: number; n: number }>>();
              data().rows.forEach((r) => {
                const arr = cohorts.get(r.cohortStart) ?? [];
                arr.push({ d: r.daysSince, n: r.retainedUsers });
                cohorts.set(r.cohortStart, arr);
              });
              return (
                <Show when={cohorts.size > 0} fallback={<Empty title="过去 N 天无注册用户分组" />}>
                  <EChart
                    height="320px"
                    option={() => {
                      const palette = [
                        cssVar('--accent', '#6366f1'),
                        cssVar('--info', '#0ea5e9'),
                        cssVar('--success', '#22c55e'),
                        cssVar('--warning', '#eab308'),
                        cssVar('--error', '#ef4444'),
                      ];
                      const series = Array.from(cohorts.entries()).slice(0, 5).map(([start, pts], i) => ({
                        name: start.slice(5), // MM-DD
                        type: 'line' as const,
                        smooth: true,
                        symbolSize: 6,
                        lineStyle: { color: palette[i % palette.length] },
                        itemStyle: { color: palette[i % palette.length] },
                        data: pts.sort((a, b) => a.d - b.d).map((p) => [p.d, p.n]),
                      }));
                      return {
                        grid: { left: 50, right: 30, top: 40, bottom: 30 },
                        legend: { top: 0, type: 'scroll', itemWidth: 14, itemHeight: 8 },
                        tooltip: { trigger: 'axis' },
                        xAxis: { type: 'value', name: '注册后天数', min: 0 },
                        yAxis: { type: 'value', name: '留存人数', minInterval: 1 },
                        series,
                      };
                    }}
                  />
                </Show>
              );
            }}
          </Show>
        </Show>
      </Card>
    </div>
  );
}
