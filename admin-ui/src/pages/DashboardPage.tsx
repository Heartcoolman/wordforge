import { createMemo, createResource, For, Show, type JSX } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';
import { Button } from '@/components/ui/Button';
import { Sparkline } from '@/components/ui/Sparkline';
import { EChart } from '@/components/ui/EChart';
import { Skeleton } from '@/components/ui/Skeleton';
import { WindowPicker } from '@/components/ui/WindowPicker';
import { Panel } from '@/components/ui/Panel';
import { MiniStat } from '@/components/ui/MiniStat';
import { WorkerGridCell, type WorkerOutcome } from '@/components/ui/WorkerGridCell';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import { uiStore } from '@/stores/ui';
import { useCountUp } from '@/lib/motion';
import { formatNumber, formatDuration, formatBytes } from '@/utils/formatters';
import type { TrendField } from '@/types/admin';

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

// m023:worker outcome 字段 → WorkerGridCell 枚举映射
function toWorkerOutcome(raw: string | null): WorkerOutcome {
  if (!raw) return 'unknown';
  const v = raw.toLowerCase();
  if (v === 'ok' || v === 'success' || v === 'healthy') return 'ok';
  if (v.includes('error') || v.includes('fail') || v === 'down') return 'error';
  if (v === 'idle' || v === 'skipped') return 'idle';
  return 'unknown';
}

const DOW_LABEL = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

export default function DashboardPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const days = createMemo(() => {
    const raw = parseInt(searchParams.days as string, 10);
    return DAYS_ALLOWED.includes(raw as typeof DAYS_ALLOWED[number]) ? raw : 7;
  });
  const setDays = (d: number) => setSearchParams({ days: d.toString() });

  // 现有资源:每个独立失败不黑屏。catch(() => null|[]) 兜底,避免某个端点 404 把整页拉下。
  const [stats, { refetch: refetchStats }] = createResource(() => adminApi.getStats());
  const [eng, { refetch: refetchEng }] = createResource(() => adminApi.getEngagement());
  const [overview, { refetch: refetchOverview }] = createResource(days, (d) => adminApi.getStudyOverview(d));
  const [dau, { refetch: refetchDau }] = createResource(days, (d) => adminApi.getDailyActiveUsers(d));
  const [records, { refetch: refetchRecords }] = createResource(days, (d) => adminApi.getDailyRecords(d));
  const [health, { refetch: refetchHealth }] = createResource(() => adminApi.getHealth());
  const [updateInfo] = createResource(() => adminApi.checkUpdate());
  const [monitoring, { refetch: refetchMonitoring }] = createResource(() =>
    amasApi.getMonitoring(20).catch(() => []),
  );

  // m023:新增 4 块资源 —— 算法分布 / 时段热图 / Worker 网格 / LLM 待审
  const [algoSeries, { refetch: refetchAlgo }] = createResource(days, (d) =>
    adminApi.amasMetricsTimeseries(d).catch(() => [] as Awaited<ReturnType<typeof adminApi.amasMetricsTimeseries>>),
  );
  const [hourly, { refetch: refetchHourly }] = createResource(days, (d) =>
    adminApi.analyticsHourly(d).catch(() => null),
  );
  const [workersResp, { refetch: refetchWorkers }] = createResource(() =>
    adminApi.monitoringWorkers().catch(() => ({ workers: [] })),
  );
  const workers = () => workersResp()?.workers ?? [];
  const [suggestions, { refetch: refetchSuggestions }] = createResource(() =>
    adminApi.amasListSuggestions('pending', 50).catch(() => []),
  );
  // 待处理事项里需要"未处理反馈"计数。perPage=1 拿 total + facets 即可,省带宽。
  // facets.byCategory/byType 由后端按 status=open 过滤聚合,与 perPage 无关,可拿到真实 bug 数。
  const [openFeedback, { refetch: refetchFeedback }] = createResource(() =>
    adminApi.listFeedback({ status: 'open', perPage: 1 }).catch(() => null),
  );

  // m023:刷新按钮 —— 触发所有 resource 重新拉取。注意 days() 变化时 days-依赖资源自动重拉,
  // 这里手动 refetch 是给那些非 days 依赖资源(stats/eng/health/etc.)用的。
  function refreshAll() {
    refetchStats();
    refetchEng();
    refetchOverview();
    refetchDau();
    refetchRecords();
    refetchHealth();
    refetchMonitoring();
    refetchAlgo();
    refetchHourly();
    refetchWorkers();
    refetchSuggestions();
    refetchFeedback();
    uiStore.toast.info('已刷新所有数据');
  }

  // m023:导出按钮 —— 把 dau + records 当前窗口的逐日序列拼 CSV 下载。
  // 没数据时也下个空表头,告知用户操作触发但本期无数据。
  function exportCsv() {
    const header = ['date', 'registered', 'active', 'records', 'correct', 'accuracy_pct', 'duration_secs', 'new_words'];
    const d = dau() ?? [];
    const r = records() ?? [];
    const max = Math.max(d.length, r.length);
    const rows: string[] = [header.join(',')];
    for (let i = 0; i < max; i += 1) {
      const di = d[i];
      const ri = r[i];
      const date = di?.date ?? ri?.date ?? '';
      const acc = ri && ri.total > 0 ? ((ri.correct / ri.total) * 100).toFixed(1) : '';
      rows.push([
        date,
        di?.registered ?? '',
        di?.count ?? '',
        ri?.total ?? '',
        ri?.correct ?? '',
        acc,
        ri?.durationSecs ?? '',
        ri?.newWords ?? '',
      ].join(','));
    }
    const blob = new Blob([rows.join('\n')], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `dashboard-${days()}d-${new Date().toISOString().slice(0, 10)}.csv`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    uiStore.toast.success(`已导出 ${max} 行 CSV`);
  }

  // 答题数 / 正确率 KPI 环比 trend chip —— 复用 study-overview summary 的 prev-period 字段。
  // 答题数走百分比环比(recordCountDeltaPct),正确率走百分点差(accuracyDeltaPt,陪文带上周基线)。
  const recordsTrend = createMemo<TrendField | null>(() => {
    const pct = overview()?.summary.recordCountDeltaPct;
    if (pct == null) return null;
    return { value: pct * 100, label: '比上周期' };
  });
  const accuracyTrend = createMemo<TrendField | null>(() => {
    const s = overview()?.summary;
    if (!s || s.accuracyDeltaPt == null) return null;
    const prev = s.prevAccuracy != null ? `${(s.prevAccuracy * 100).toFixed(1)}%` : '上周期';
    const up = s.accuracyDeltaPt >= 0;
    // 正确率用百分点(pt)语义,trend.value 复用为 pt 值,文案点明高/低于上周基线
    return { value: s.accuracyDeltaPt * 100, label: `${up ? '高于' : '低于'}上周 ${prev}` };
  });

  // sparkline 序列
  const registeredSpark = createMemo(() => dau()?.map((d) => d.registered) ?? []);
  const activeSpark = createMemo(() => dau()?.map((d) => d.count) ?? []);
  const recordsSpark = createMemo(() => records()?.map((d) => d.total) ?? []);
  const accuracySpark = createMemo(
    () => records()?.map((d) => (d.total > 0 ? (d.correct / d.total) * 100 : 0)) ?? [],
  );

  // m023:算法分布 —— 把 N 天的 timeseries 按 algorithm reduce + 取 top 6,算占比
  const algoDistribution = createMemo(() => {
    const points = algoSeries() ?? [];
    if (points.length === 0) return { rows: [] as Array<{ name: string; count: number; pct: number }>, total: 0 };
    const agg = new Map<string, number>();
    let total = 0;
    for (const p of points) {
      agg.set(p.algorithm, (agg.get(p.algorithm) ?? 0) + p.callCount);
      total += p.callCount;
    }
    if (total === 0) return { rows: [], total: 0 };
    const rows = Array.from(agg.entries())
      .map(([name, count]) => ({ name, count, pct: (count / total) * 100 }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 6);
    return { rows, total };
  });

  // 待处理事项 sub-copy 真实数据派生 —— 仅用已有资源真实字段,无源时降级为通用文案(不编造)。
  // 反馈:后端 list 端点未透出按 category 的 facets,故 sub-copy 保持通用描述(honest-degradation)。
  // LLM 顾问:汇总 pending 建议成本($,与卡片内 costUsd 同单位),无成本时降级。
  const advisorTodoDesc = createMemo(() => {
    const list = suggestions() ?? [];
    if (list.length === 0) return 'approve 后会进入灰度发布';
    const cost = list.reduce((a, s) => a + (s.costUsd ?? 0), 0);
    return cost > 0 ? `approve 后进入灰度 · 本批调参成本 $${cost.toFixed(4)}` : 'approve 后会进入灰度发布';
  });

  // m023:监控告警(error/warn 级别) —— 仅用于"待处理事项"行计数,不再独立成卡
  const alertCount = createMemo(() => {
    const events = monitoring() ?? [];
    return events.filter((e) => /error|warn|fail|denied|critical/i.test(e.eventType)).length;
  });

  // 全失败兜底 banner
  const allFailed = createMemo(
    () => !!(stats.error && eng.error && overview.error && dau.error && records.error && health.error),
  );

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

  // m023:Worker 健康统计 —— "16 健康 · 1 异常"摘要。
  // 后端 WorkerStatusRow 仅透出 lastOutcome,无心跳阈值字段;故"警告"语义按 outcome 派生,
  // 设计图的「心跳超阈值」告警因后端未透出阈值/staleness,此处不编造(honest-degradation)。
  const workerSummary = createMemo(() => {
    const ws = workers();
    let ok = 0;
    let err = 0;
    let other = 0;
    for (const w of ws) {
      const o = toWorkerOutcome(w.lastOutcome);
      if (o === 'error') err += 1;
      else if (o === 'ok') ok += 1;
      else other += 1;
    }
    return { ok, err, other, total: ws.length };
  });

  // outcome=error 的具名 worker —— 用于「待处理事项」具名告警行 + 直达探针/监控排查。
  const failedWorker = createMemo(() => {
    const ws = workers().filter((w) => toWorkerOutcome(w.lastOutcome) === 'error');
    if (ws.length === 0) return null;
    const w = ws[0];
    return { name: w.workerName, error: w.lastError, extra: ws.length - 1 };
  });

  return (
    <div class="space-y-6">
      {/* Page header — 简洁标题 + desc + 右侧 WindowPicker / 刷新 / 导出。
          对齐设计图 dashboard.html .page-header,不再用 HeroCard 圆角卡 + halo 渐变。 */}
      <header class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-0">
          <h1 class="text-[clamp(26px,3vw,34px)] font-bold tracking-tight text-content">全局概览</h1>
          <p class="mt-2 text-sm leading-relaxed text-content-secondary max-w-2xl">
            注册 / 活跃 / 答题 / 健康一屏看完。所有数据来自 axum{' '}
            <code class="font-mono text-content text-[12.5px] px-1 py-0.5 rounded bg-surface-sunken">/admin/stats</code>
            {' '}与{' '}
            <code class="font-mono text-content text-[12.5px] px-1 py-0.5 rounded bg-surface-sunken">/admin/engagement</code>
            {' '}的实时聚合。
          </p>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <WindowPicker value={days()} onChange={setDays} />
          <Button
            variant="secondary"
            size="md"
            onClick={refreshAll}
            icon={
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
              </svg>
            }
          >
            刷新
          </Button>
          <Button
            variant="secondary"
            size="md"
            onClick={exportCsv}
            icon={
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                <polyline points="7 10 12 15 17 10" />
                <line x1="12" y1="15" x2="12" y2="3" />
              </svg>
            }
          >
            导出
          </Button>
        </div>
      </header>

      <Show when={allFailed()}>
        <div
          role="alert"
          class="flex items-center gap-2.5 rounded-lg border border-error/30 bg-error-light px-4 py-3 text-error-strong text-sm"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
            <circle cx="12" cy="12" r="10" /><path d="M12 8v4" /><path d="M12 16h.01" />
          </svg>
          <span>加载失败:所有数据源均不可达,请检查后端 /api/admin/* 或网络。</span>
        </div>
      </Show>

      {/* KPI 4 卡 — tint 底 + icon/label 同行 + 大数字 + trend chip + 右下 sparkline。
          对齐 dashboard.html .kpi.is-{accent|success|info|warning}。 */}
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <Show
          when={stats()}
          fallback={stats.error ? <KpiErrorCell onRetry={refetchStats} /> : <Skeleton height="104px" />}
        >
          {(s) => (
            <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[0]}>
              <KpiCard
                tone="accent"
                title="注册用户"
                icon={ICON_USERS}
                value={s().users}
                format={formatNumber}
                trend={s().trend?.users ?? null}
                spark={registeredSpark()}
              />
            </div>
          )}
        </Show>
        <Show
          when={eng()}
          fallback={eng.error ? <KpiErrorCell onRetry={refetchEng} /> : <Skeleton height="104px" />}
        >
          {(e) => (
            <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[1]}>
              <KpiCard
                tone="success"
                title="今日活跃"
                icon={ICON_LIGHTNING}
                value={e().activeToday}
                format={formatNumber}
                trend={e().trend?.activeToday ?? null}
                spark={activeSpark()}
              />
            </div>
          )}
        </Show>
        <Show
          when={overview()}
          fallback={overview.error ? <KpiErrorCell onRetry={refetchOverview} /> : <Skeleton height="104px" />}
        >
          {(o) => (
            <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[2]}>
              <KpiCard
                tone="info"
                title={`${days()} 天答题数`}
                icon={ICON_CALENDAR}
                value={o().summary.recordCount}
                format={formatNumber}
                trend={recordsTrend()}
                spark={recordsSpark()}
              />
            </div>
          )}
        </Show>
        <Show
          when={overview()}
          fallback={overview.error ? <KpiErrorCell onRetry={refetchOverview} /> : <Skeleton height="104px" />}
        >
          {(o) => (
            <div class="animate-fade-in-up h-full" style={STAGGER_DELAYS[3]}>
              <KpiCard
                tone="warning"
                title={`${days()} 天正确率`}
                icon={ICON_CHECK_CIRCLE}
                value={o().summary.accuracy != null ? Number((o().summary.accuracy! * 100).toFixed(1)) : '—'}
                unit={o().summary.accuracy != null ? '%' : undefined}
                trend={accuracyTrend()}
                spark={accuracySpark()}
              />
            </div>
          )}
        </Show>
      </div>

      {/* Row A:用户活跃趋势(8) + AMAS 算法分布(4) */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-6">
        <div class="lg:col-span-8 min-w-0">
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
                      // 对齐 dashboard.html:日活跃=accent 主线,新注册=success 绿副线
                      const accent = cssVar('--accent', '#6366f1');
                      const success = cssVar('--success', '#10b981');
                      return {
                        grid: { left: 40, right: 20, top: 30, bottom: 30 },
                        legend: { data: ['日活跃', '新注册'], top: 0, icon: 'roundRect', itemWidth: 8, itemHeight: 8 },
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
                            lineStyle: { color: accent },
                            areaStyle: { opacity: 0.18 },
                          },
                          {
                            name: '新注册',
                            type: 'line',
                            data: data().map((d) => d.registered),
                            smooth: true,
                            itemStyle: { color: success },
                            lineStyle: { color: success },
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
        </div>

        {/* m023:AMAS 算法决策分布 —— 用 amasMetricsTimeseries N 天聚合 + top 6 占比 */}
        <div class="lg:col-span-4 min-w-0">
          <Card variant="elevated" class="h-full flex flex-col">
            <div class="flex items-start justify-between pb-3 mb-3 border-b border-border-hairline">
              <div>
                <h2 class="text-headline text-content">AMAS 算法分布</h2>
                <p class="font-mono text-[11.5px] text-content-tertiary mt-1">
                  {days()} 天 · {formatNumber(algoDistribution().total)} 次决策
                </p>
              </div>
              <a href="/admin/amas-metrics" class="text-[12px] text-content-tertiary hover:text-accent">
                详情 →
              </a>
            </div>
            <Show
              when={!algoSeries.loading}
              fallback={<Skeleton height="220px" />}
            >
              <Show
                when={algoDistribution().rows.length > 0}
                fallback={<Empty title="暂无数据" description="决策事件未上报" />}
              >
                <ul class="flex flex-col gap-2.5">
                  <For each={algoDistribution().rows}>
                    {(row, i) => (
                      <li class="grid grid-cols-[7ch_1fr_5ch] items-center gap-2.5 text-[12.5px]">
                        <span class="text-content-secondary font-mono truncate">{row.name}</span>
                        <div class="h-2 rounded-full bg-surface-sunken overflow-hidden">
                          <div
                            class="h-full rounded-full transition-[width] duration-500 ease-out"
                            style={{
                              width: `${row.pct.toFixed(1)}%`,
                              background: `oklch(${58 + i() * 2}% 0.18 ${(269 + i() * 40) % 360})`,
                            }}
                          />
                        </div>
                        <span class="font-mono text-content tabular-nums text-right">
                          {row.pct.toFixed(1)}%
                        </span>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </Show>
          </Card>
        </div>
      </div>

      {/* Row B:学习产出(8) + 24h × 7d 学习时段热图(4) */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-6">
        <div class="lg:col-span-8 min-w-0">
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
        </div>

        {/* m023:24h × 7d 学习时段热图 —— 复用 analyticsHourly matrix[7][24] */}
        <div class="lg:col-span-4 min-w-0">
          <Card variant="elevated" class="h-full flex flex-col">
            <div class="flex items-start justify-between pb-3 mb-3 border-b border-border-hairline">
              <div>
                <h2 class="text-headline text-content">用户学习时段</h2>
                <p class="font-mono text-[11.5px] text-content-tertiary mt-1">24h × 7d 热图</p>
              </div>
            </div>
            <Show
              when={!hourly.loading}
              fallback={<Skeleton height="220px" />}
            >
              <Show
                when={hourly()}
                fallback={<Empty title="暂无数据" description="未生成学习时段数据" />}
              >
                {(h) => (
                  <EChart
                    height="220px"
                    option={() => {
                      const matrix = h().matrix; // [7][24]
                      const accent = cssVar('--accent', '#6366f1');
                      const points: Array<[number, number, number]> = [];
                      let max = 0;
                      for (let dow = 0; dow < matrix.length; dow += 1) {
                        const row = matrix[dow] ?? [];
                        for (let hour = 0; hour < row.length; hour += 1) {
                          const v = row[hour] ?? 0;
                          points.push([hour, dow, v]);
                          if (v > max) max = v;
                        }
                      }
                      return {
                        grid: { left: 40, right: 10, top: 10, bottom: 30 },
                        tooltip: {
                          position: 'top',
                          formatter: (p: unknown) => {
                            const data = (p as { data: [number, number, number] }).data;
                            return `${DOW_LABEL[data[1]]} ${String(data[0]).padStart(2, '0')}:00 · ${data[2]} 次`;
                          },
                        },
                        xAxis: {
                          type: 'category',
                          data: Array.from({ length: 24 }, (_, i) => `${i}`),
                          splitArea: { show: true },
                        },
                        yAxis: { type: 'category', data: DOW_LABEL, splitArea: { show: true } },
                        visualMap: {
                          min: 0,
                          max: max || 1,
                          calculable: false,
                          orient: 'horizontal',
                          left: 'center',
                          bottom: 0,
                          show: false,
                          inRange: { color: ['transparent', accent] },
                        },
                        series: [
                          {
                            type: 'heatmap',
                            data: points,
                            itemStyle: { borderRadius: 2 },
                          },
                        ],
                      };
                    }}
                  />
                )}
              </Show>
            </Show>
          </Card>
        </div>
      </div>

      {/* Row C:系统状态(5) + Worker 心跳网格(7) */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-6">
        <div class="lg:col-span-5 min-w-0">
          <Show when={health()} fallback={<Card variant="elevated"><Skeleton height="280px" /></Card>}>
            {(h) => (
              <Card variant="elevated">
                <h2 class="text-lg font-semibold text-content mb-3">系统状态</h2>
                <div class="grid grid-cols-2 gap-3 text-sm">
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
                        {h().status === 'healthy' ? '运行正常' : h().status === 'degraded' ? '性能降级' : '服务异常'}
                      </span>
                    </p>
                  </div>
                  <div>
                    <p class="text-content-secondary">数据库</p>
                    <p class="font-medium text-content tabular-nums">{formatBytes(h().dbSizeBytes)}</p>
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

                {/* m023:进程资源进度条 —— 仅在后端 resources 字段存在时渲染,失败兜底直接隐藏 */}
                <Show when={h().resources}>
                  {(res) => (
                    <div class="mt-4 pt-4 border-t border-border-hairline flex flex-col gap-2.5">
                      <Show when={res().cpuPct != null}>
                        <ResourceBar
                          label="CPU 使用"
                          value={`${res().cpuPct!.toFixed(1)}%`}
                          pct={Math.min(res().cpuPct!, 100)}
                        />
                      </Show>
                      <Show when={res().memoryRssBytes != null}>
                        <ResourceBar
                          label="内存 (RSS)"
                          value={formatBytes(res().memoryRssBytes!)}
                          pct={null}
                        />
                      </Show>
                      <Show when={res().pool}>
                        {(p) => (
                          <ResourceBar
                            label="DB 连接池"
                            value={`${p().connections} / ${p().max}`}
                            pct={p().max > 0 ? (p().connections / p().max) * 100 : null}
                            tone={p().max > 0 && p().connections / p().max > 0.8 ? 'warning' : 'success'}
                          />
                        )}
                      </Show>
                      <Show when={res().diskTotalBytes != null && res().diskFreeBytes != null}>
                        <ResourceBar
                          label="磁盘"
                          value={`${formatBytes(res().diskTotalBytes! - res().diskFreeBytes!)} / ${formatBytes(res().diskTotalBytes!)}`}
                          pct={
                            res().diskTotalBytes! > 0
                              ? ((res().diskTotalBytes! - res().diskFreeBytes!) / res().diskTotalBytes!) * 100
                              : null
                          }
                        />
                      </Show>
                    </div>
                  )}
                </Show>
              </Card>
            )}
          </Show>
        </div>

        {/* m023:Worker 心跳网格 */}
        <div class="lg:col-span-7 min-w-0">
          <Card variant="elevated" class="h-full flex flex-col">
            <div class="flex items-start justify-between pb-3 mb-3 border-b border-border-hairline">
              <div>
                <h2 class="text-headline text-content">Worker 心跳</h2>
                <p class="font-mono text-[11.5px] text-content-tertiary mt-1">
                  {workerSummary().total} 个 tokio worker
                </p>
              </div>
              <a href="/admin/monitoring" class="text-[12px] text-content-tertiary hover:text-accent">
                完整监控 →
              </a>
            </div>
            <Show
              when={!workersResp.loading}
              fallback={<Skeleton height="220px" />}
            >
              <Show
                when={workers().length > 0}
                fallback={<Empty title="暂无 worker" description="后端未注册 worker 心跳" />}
              >
                <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-3 xl:grid-cols-4 gap-2">
                  <For each={workers()}>
                    {(w) => (
                      <WorkerGridCell
                        name={w.workerName}
                        outcome={toWorkerOutcome(w.lastOutcome)}
                        lastRunAt={w.lastRunAt}
                        lastDurationMs={w.lastDurationMs}
                        lastError={w.lastError}
                      />
                    )}
                  </For>
                </div>
                <Show when={workerSummary().total > 0}>
                  <div class="mt-3 pt-3 border-t border-border-hairline flex items-center justify-between gap-3 text-[12px] text-content-secondary">
                    <span>
                      <span class="text-success-strong tabular-nums">{workerSummary().ok}</span> 健康
                      <Show when={workerSummary().err > 0}>
                        {' · '}
                        <span class="text-error-strong tabular-nums">{workerSummary().err}</span> 异常
                        <Show when={failedWorker()}>
                          {(f) => (
                            <span class="text-content-tertiary">（{f().name}{f().extra > 0 ? ` 等 ${f().extra + 1} 个` : ''}）</span>
                          )}
                        </Show>
                      </Show>
                    </span>
                    <Show when={workerSummary().err > 0}>
                      <a href="/admin/probe" class="shrink-0 text-warning-strong hover:underline">
                        查看探针 →
                      </a>
                    </Show>
                  </div>
                </Show>
              </Show>
            </Show>
          </Card>
        </div>
      </div>

      {/* Row D:LLM 顾问待审 preview(7) + 待处理事项 4 源聚合(5) */}
      <div class="grid grid-cols-1 lg:grid-cols-12 gap-6">
        <div class="lg:col-span-7 min-w-0">
          <Card variant="elevated" class="h-full flex flex-col">
            <div class="flex items-start justify-between pb-3 mb-3 border-b border-border-hairline">
              <div>
                <h2 class="text-headline text-content">LLM 调参顾问 · 待审建议</h2>
                <p class="font-mono text-[11.5px] text-content-tertiary mt-1">
                  pending {suggestions()?.length ?? 0} 条
                </p>
              </div>
              <a href="/admin/amas-advisor" class="text-[12px] text-content-tertiary hover:text-accent">
                前往顾问 →
              </a>
            </div>
            <Show
              when={!suggestions.loading}
              fallback={<Skeleton height="180px" />}
            >
              <Show
                when={(suggestions() ?? []).length > 0}
                fallback={
                  <div class="flex items-center gap-2 py-4 text-[13px] text-content-tertiary">
                    <span class="inline-block size-1.5 rounded-full bg-success animate-ring-pulse" aria-hidden="true" />
                    暂无待审建议,LLM advisor 未给出新提案。
                  </div>
                }
              >
                <ul class="flex flex-col divide-y divide-border-hairline">
                  <For each={(suggestions() ?? []).slice(0, 2)}>
                    {(s) => (
                      <li class="py-3 first:pt-0 last:pb-0">
                        <div class="flex items-start justify-between gap-3">
                          <p class="text-[13.5px] font-medium text-content min-w-0 flex-1">{s.rationale}</p>
                          <span class="shrink-0 inline-flex items-center px-2 py-0.5 rounded-pill text-[11px] font-medium bg-warning-light text-warning-strong">
                            待审
                          </span>
                        </div>
                        <div class="mt-2 flex items-center gap-2 flex-wrap text-[12px]">
                          <span class="font-mono text-content-tertiary">
                            patch {Object.keys(s.patchJson).length} 项
                          </span>
                          <Show when={s.confidence != null}>
                            <span class="text-content-tertiary">
                              · 置信度 <span class="font-mono tabular-nums">{(s.confidence! * 100).toFixed(0)}%</span>
                            </span>
                          </Show>
                          <Show when={s.costUsd != null}>
                            <span class="text-content-tertiary">
                              · 成本 <span class="font-mono tabular-nums">${s.costUsd!.toFixed(4)}</span>
                            </span>
                          </Show>
                          <a
                            href="/admin/amas-advisor"
                            class="ml-auto text-accent hover:underline"
                          >
                            查看 diff →
                          </a>
                        </div>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </Show>
          </Card>
        </div>

        {/* m023:待处理事项 —— 4 源聚合(新版本/未处理反馈/告警/LLM 待审) */}
        <div class="lg:col-span-5 min-w-0">
          <Card variant="elevated" class="h-full flex flex-col">
            <div class="flex items-start justify-between pb-3 mb-3 border-b border-border-hairline">
              <div>
                <h2 class="text-headline text-content">待处理事项</h2>
                <p class="font-mono text-[11.5px] text-content-tertiary mt-1">需要管理员关注</p>
              </div>
            </div>
            <ul class="flex flex-col divide-y divide-border-hairline -mx-2">
              <Show when={updateInfo()?.hasUpdate}>
                <TodoRow
                  href={updateInfo()!.releaseUrl ?? '/admin/updates'}
                  iconTone="accent"
                  title={`新版本 ${updateInfo()!.latestVersion} 可用`}
                  desc="点击查看发行说明并升级"
                  external={!!updateInfo()!.releaseUrl}
                />
              </Show>
              <Show when={(openFeedback()?.total ?? 0) > 0}>
                <TodoRow
                  href="/admin/feedback?status=open"
                  iconTone="warning"
                  title={`${openFeedback()!.total} 条未处理反馈`}
                  desc="包含 bug 报告与功能请求"
                />
              </Show>
              {/* worker outcome=error —— 具名告警行,destination 直达探针(对齐设计图 probe 项) */}
              <Show when={failedWorker()}>
                {(f) => (
                  <TodoRow
                    href="/admin/probe"
                    iconTone="error"
                    title={`Worker ${f().name} 执行异常${f().extra > 0 ? ` 等 ${f().extra + 1} 个` : ''}`}
                    desc={f().error ?? '最近一次执行返回 error,请前往探针排查'}
                  />
                )}
              </Show>
              <Show when={alertCount() > 0}>
                <TodoRow
                  href="/admin/monitoring"
                  iconTone="warning"
                  title={`${alertCount()} 条监控告警`}
                  desc="最近 20 条事件含 error / warn"
                />
              </Show>
              <Show when={(suggestions()?.length ?? 0) > 0}>
                <TodoRow
                  href="/admin/amas-advisor"
                  iconTone="accent"
                  title={`LLM 顾问 · ${suggestions()!.length} 条建议待审`}
                  desc={advisorTodoDesc()}
                />
              </Show>
              <Show
                when={
                  !updateInfo()?.hasUpdate &&
                  (openFeedback()?.total ?? 0) === 0 &&
                  !failedWorker() &&
                  alertCount() === 0 &&
                  (suggestions()?.length ?? 0) === 0 &&
                  !openFeedback.loading &&
                  !suggestions.loading
                }
              >
                <li class="px-2 py-4 flex items-center gap-2 text-[13px] text-content-tertiary">
                  <span class="inline-block size-1.5 rounded-full bg-success animate-ring-pulse" aria-hidden="true" />
                  当前无待办,系统运行平稳。
                </li>
              </Show>
            </ul>
          </Card>
        </div>
      </div>
    </div>
  );
}

// ─────────── 局部辅助组件 ───────────

// dashboard 4 张 KPI 卡的 icon path(viewBox 24×24,stroke 路径)
const ICON_USERS =
  'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 7a4 4 0 1 0 0 8 4 4 0 0 0 0-8z';
const ICON_LIGHTNING = 'M13 2L3 14h9l-1 8 10-12h-9l1-8z';
const ICON_CALENDAR =
  'M19 4H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2zM16 2v4M8 2v4M3 10h18';
const ICON_CHECK_CIRCLE = 'M22 11.08V12a10 10 0 1 1-5.93-9.14M22 4L12 14.01l-3-3';

// 4 色对应 CSS var,renderer 内联到 background gradient + sparkline stroke
const KPI_TONE: Record<
  'accent' | 'success' | 'info' | 'warning',
  { var: string; labelClass: string }
> = {
  accent: { var: '--accent', labelClass: 'text-accent' },
  success: { var: '--success', labelClass: 'text-success-strong' },
  info: { var: '--info', labelClass: 'text-info-strong' },
  warning: { var: '--warning', labelClass: 'text-warning-strong' },
};

interface KpiCardProps {
  tone: 'accent' | 'success' | 'info' | 'warning';
  title: string;
  icon: string;
  value: string | number;
  unit?: string;
  format?: (n: number) => string;
  /** 上/下行 chip + 文字。null 时不渲染 chip 行 */
  trend?: TrendField | null;
  /** sparkline 数据,长度 ≥2 时渲染右下 */
  spark?: number[];
}

/**
 * Dashboard 顶部 KPI 卡 —— 像素对齐 admin.css .kpi.is-{tone}:
 *
 * - background: linear-gradient(135deg, var(--{tone}-light) 0%, var(--surface-elevated) 60%)
 * - padding: 16px 18px  (px-[18px] py-4)
 * - kpi-value: 700 28px/1.05  (text-[28px] leading-[1.05])
 * - kpi-spark: absolute right:14 bottom:14, 80×28, opacity 0.65
 * - 不设 min-height,卡片按内容自然收缩;grid stretch 让 4 张同高
 *
 * 数据为 number 时启用 count-up 动画;'—' 等 string 直显。
 */
function KpiCard(props: KpiCardProps): JSX.Element {
  const tone = () => KPI_TONE[props.tone];
  const numericTarget = createMemo(() => (typeof props.value === 'number' ? props.value : null));
  const animated = useCountUp({
    to: () => numericTarget() ?? 0,
    format: props.format,
  });
  const displayValue = () => (numericTarget() != null ? animated() : String(props.value));

  const trendUp = () => (props.trend?.value ?? 0) > 0;
  const trendDown = () => (props.trend?.value ?? 0) < 0;
  const hasTrend = () => !!props.trend && props.trend.value !== 0;

  return (
    <div
      class="relative overflow-hidden rounded-lg shadow-elevation-1 px-[18px] py-4 h-full"
      style={{
        background: `linear-gradient(135deg, var(${tone().var}-light) 0%, var(--surface-elevated) 60%)`,
      }}
    >
      {/* icon + label 同行,12px medium —— admin.css .kpi-label */}
      <div class={`flex items-center gap-1.5 text-[12px] font-medium ${tone().labelClass}`}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
          <path d={props.icon} stroke-linecap="round" stroke-linejoin="round" />
        </svg>
        <span>{props.title}</span>
      </div>

      {/* 大数字 700 28px/1.05 —— admin.css .kpi-value */}
      <div class="mt-1.5 text-[28px] leading-[1.05] font-bold tabular-nums tracking-[-0.025em] text-content">
        {displayValue()}
        <Show when={props.unit}>
          <span class="ml-1 text-sm font-medium text-content-tertiary">{props.unit}</span>
        </Show>
      </div>

      {/* trend chip + 文字 —— admin.css .kpi-trend, .delta.up/.down */}
      <Show when={hasTrend()}>
        <div class="mt-1.5 flex items-center gap-1 text-[11.5px] text-content-tertiary">
          <span
            class={`inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded font-semibold tabular-nums ${
              trendUp()
                ? 'bg-success-light text-success-strong'
                : trendDown()
                ? 'bg-error-light text-error-strong'
                : 'bg-surface-secondary text-content-tertiary'
            }`}
          >
            {trendUp() ? '▲' : trendDown() ? '▼' : '—'} {Math.abs(props.trend!.value).toFixed(1)}%
          </span>
          <span class="truncate">{props.trend!.label}</span>
        </div>
      </Show>

      {/* sparkline absolute 右下 80×28 opacity 0.65 —— admin.css .kpi-spark */}
      <Show when={props.spark && props.spark.length >= 2}>
        <div class="absolute right-[14px] bottom-[14px] pointer-events-none opacity-65">
          <Sparkline data={props.spark!} w={80} h={28} stroke={`var(${tone().var})`} strokeWidth={1.5} />
        </div>
      </Show>
    </div>
  );
}

interface ResourceBarProps {
  label: string;
  value: string;
  /** 0–100 占比,null 时不渲染进度条只显示数值 */
  pct: number | null;
  tone?: 'accent' | 'success' | 'warning';
}

function ResourceBar(props: ResourceBarProps) {
  const tone = () => props.tone ?? 'accent';
  const bg = () =>
    tone() === 'success' ? 'bg-success' : tone() === 'warning' ? 'bg-warning' : 'bg-accent';
  return (
    <div class="flex flex-col gap-1">
      <div class="flex items-center justify-between text-[12px]">
        <span class="text-content-secondary">{props.label}</span>
        <span class="font-mono text-content tabular-nums">{props.value}</span>
      </div>
      <Show when={props.pct != null}>
        <div class="h-1.5 rounded-full bg-surface-sunken overflow-hidden">
          <div
            class={`h-full rounded-full transition-[width] duration-500 ease-out ${bg()}`}
            style={{ width: `${Math.min(Math.max(props.pct!, 0), 100)}%` }}
          />
        </div>
      </Show>
    </div>
  );
}

interface TodoRowProps {
  href: string;
  iconTone: 'accent' | 'warning' | 'error';
  title: string;
  desc: string;
  external?: boolean;
}

function TodoRow(props: TodoRowProps) {
  const bgClass = () =>
    props.iconTone === 'warning'
      ? 'bg-warning-light text-warning-strong'
      : props.iconTone === 'error'
      ? 'bg-error-light text-error-strong'
      : 'bg-accent-light text-accent';
  return (
    <li>
      <a
        href={props.href}
        target={props.external ? '_blank' : undefined}
        rel={props.external ? 'noopener noreferrer' : undefined}
        class="flex items-center gap-3 px-2 py-2.5 rounded transition-colors hover:bg-surface-sunken"
      >
        <span class={`size-8 rounded-lg grid place-items-center shrink-0 ${bgClass()}`}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
            <path d="M9 18l6-6-6-6" />
          </svg>
        </span>
        <div class="min-w-0 flex-1">
          <p class="text-[13px] font-medium text-content truncate">{props.title}</p>
          <p class="text-[11.5px] text-content-tertiary truncate">{props.desc}</p>
        </div>
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          class="text-content-tertiary shrink-0"
          aria-hidden="true"
        >
          <path d="M9 18l6-6-6-6" />
        </svg>
      </a>
    </li>
  );
}
