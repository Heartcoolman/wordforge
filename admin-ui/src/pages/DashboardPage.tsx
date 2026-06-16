import { createMemo, createResource, createSignal, For, Show, onCleanup, startTransition, type JSX } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import {
  Btn, Badge, Panel, StatCard, Empty, Loading, Skel, Progress, Icon,
  LineChart, Donut, Heatmap, Seg, sx, cx, fmtNum, fmtBytes, fmtDur, fmtAgo, toast,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import type {
  AdminStats, EngagementAnalytics, StudyOverview, DailyActiveUsersEntry,
  DailyRecordsEntry, SystemHealth, UpdateCheck, WorkerStatusRow,
} from '@/types/admin';
import type { AmasMetricsTimeseriesPoint, AmasSuggestion } from '@/api/admin';
import type { MonitoringEvent } from '@/types/amas';

// 算法甜甜圈固定调色板（对齐设计稿 ALGO_COLORS）
const ALGO_COLORS = ['#7c5cff', '#4f7dfb', '#18a558', '#d99411', '#e5484d', '#0ea5b7', '#a855f7', '#64748b'];
const DOW = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];
const DAYS_OPTS = [{ value: '7', label: '7 天' }, { value: '14', label: '14 天' }, { value: '30', label: '30 天' }];

/* ============================================================
   局部辅助组件 —— 镜像设计稿 MiniStat / KV / ResBar / TodoRow
   ============================================================ */

const TONE_COL: Record<string, string> = {
  accent: 'var(--accent)', success: 'var(--success)', info: 'var(--info)',
  warning: 'var(--warning)', error: 'var(--error)',
};

function MiniStat(props: { label: string; value: string; tone: keyof typeof TONE_COL }) {
  return (
    <div style={sx({ textAlign: 'right' })}>
      <div class="muted-3" style={sx({ fontSize: 10.5 })}>{props.label}</div>
      <div class="tnum" style={sx({ fontSize: 17, fontWeight: 700, color: TONE_COL[props.tone] })}>{props.value}</div>
    </div>
  );
}

function KV(props: { label: string; value?: JSX.Element | string; mono?: boolean; children?: JSX.Element }) {
  return (
    <div>
      <div class="muted-3" style={sx({ fontSize: 11.5, marginBottom: 3 })}>{props.label}</div>
      <div class={cx(props.mono && 'mono')} style={sx({ fontWeight: 600, fontSize: 13.5 })}>
        {props.value ?? props.children}
      </div>
    </div>
  );
}

function ResBar(props: {
  label: string; value: string; pct: number | null; tone?: 'accent' | 'success' | 'warning' | 'error' | 'info';
}) {
  const tone = () => props.tone ?? ((props.pct ?? 0) > 80 ? 'warning' : 'accent');
  return (
    <div>
      <div style={sx({ display: 'flex', justifyContent: 'space-between', fontSize: 11.5, marginBottom: 4 })}>
        <span class="muted">{props.label}</span>
        <span class="mono" style={sx({ fontWeight: 600 })}>{props.value}</span>
      </div>
      <Show when={props.pct != null} fallback={<div class="bar" style={sx({ height: 6, opacity: 0.4 })}><i style={sx({ width: '0%' })} /></div>}>
        <Progress value={Math.min(Math.max(props.pct!, 0), 100)} tone={tone()} height={6} />
      </Show>
    </div>
  );
}

function TodoRow(props: {
  tone: 'accent' | 'warning' | 'error'; icon: string; title: string; desc: string; onClick: () => void;
}) {
  const col = () => TONE_COL[props.tone];
  return (
    <button
      onClick={() => props.onClick()}
      style={sx({
        display: 'flex', gap: 11, alignItems: 'center', padding: '11px 4px', border: 'none',
        borderBottom: '1px solid var(--hairline)', background: 'transparent', cursor: 'pointer',
        textAlign: 'left', width: '100%',
      })}
    >
      <span style={sx({
        display: 'grid', placeItems: 'center', width: 30, height: 30, borderRadius: 9, flex: 'none',
        background: `color-mix(in oklch, ${col()} 14%, transparent)`, color: col(),
      })}>
        <Icon name={props.icon} size={15} />
      </span>
      <span style={sx({ minWidth: 0, flex: 1 })}>
        <span style={sx({ display: 'block', fontWeight: 600, fontSize: 13 })}>{props.title}</span>
        <span class="muted-3" style={sx({ display: 'block', fontSize: 11.5, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{props.desc}</span>
      </span>
      <Icon name="chevR" size={15} />
    </button>
  );
}

/* ============================================================
   Dashboard — 全局概览
   ============================================================ */

const DAYS_ALLOWED = new Set([7, 14, 30]);

export default function DashboardPage() {
  const navigate = useNavigate();
  const [daysStr, setDaysStr] = createSignal('7');
  const days = createMemo(() => {
    const n = parseInt(daysStr(), 10);
    return DAYS_ALLOWED.has(n) ? n : 7;
  });

  // 非窗口资源
  const [stats, { refetch: refetchStats }] = createResource<AdminStats>(() => adminApi.getStats());
  const [eng, { refetch: refetchEng }] = createResource<EngagementAnalytics>(() => adminApi.getEngagement());
  const [health, { refetch: refetchHealth }] = createResource<SystemHealth>(() => adminApi.getHealth());
  const [updateInfo, { refetch: refetchUpdate }] = createResource<UpdateCheck | null>(() =>
    adminApi.checkUpdate().catch(() => null),
  );
  const [suggestions, { refetch: refetchSuggestions }] = createResource<AmasSuggestion[]>(() =>
    adminApi.amasListSuggestions('pending').catch(() => []),
  );
  const [workersResp, { refetch: refetchWorkers }] = createResource<{ workers: WorkerStatusRow[] }>(() =>
    adminApi.monitoringWorkers().catch(() => ({ workers: [] })),
  );
  const [openFeedback, { refetch: refetchFeedback }] = createResource<{ total: number } | null>(() =>
    adminApi.listFeedback({ status: 'open', perPage: 1 }).catch(() => null),
  );
  const [monitoring, { refetch: refetchMonitoring }] = createResource<MonitoringEvent[]>(() =>
    amasApi.getMonitoring(20).catch(() => []),
  );

  // 窗口型资源（days 变化自动重拉）
  const [overview, { refetch: refetchOverview }] = createResource<StudyOverview, number>(days, (d) =>
    adminApi.getStudyOverview(d),
  );
  const [dau, { refetch: refetchDau }] = createResource<DailyActiveUsersEntry[], number>(days, (d) =>
    adminApi.getDailyActiveUsers(d),
  );
  const [records, { refetch: refetchRecords }] = createResource<DailyRecordsEntry[], number>(days, (d) =>
    adminApi.getDailyRecords(d),
  );
  const [hourly, { refetch: refetchHourly }] = createResource<{ matrix: number[][] } | null, number>(days, (d) =>
    adminApi.analyticsHourly(d).catch(() => null),
  );
  const [algoSeries, { refetch: refetchAlgo }] = createResource<AmasMetricsTimeseriesPoint[], number>(days, (d) =>
    adminApi.amasMetricsTimeseries(d).catch(() => []),
  );

  // 5s 轮询刷新 worker 心跳（live 面板）
  // 关键：refetch 在页级 <Suspense> 下会触发 PageSpinner fallback → 整页重挂载 → 滚动归顶。
  // 用 startTransition 包裹：transition 中 refetch 不触发 fallback，保留旧内容直至新数据就绪。
  const heartbeat = setInterval(() => { void startTransition(() => refetchWorkers()); }, 5000);
  onCleanup(() => clearInterval(heartbeat));

  function reloadAll() {
    void startTransition(() => {
      refetchStats(); refetchEng(); refetchHealth(); refetchUpdate(); refetchSuggestions();
      refetchWorkers(); refetchFeedback(); refetchMonitoring();
      refetchOverview(); refetchDau(); refetchRecords(); refetchHourly(); refetchAlgo();
    });
    toast.info('已刷新所有数据');
  }

  // CSV 导出当前窗口逐日序列
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
        date, di?.registered ?? '', di?.count ?? '', ri?.total ?? '', ri?.correct ?? '',
        acc, ri?.durationSecs ?? '', ri?.newWords ?? '',
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
    toast.success(`已导出 ${max} 行 CSV`);
  }

  // ─────────── 派生数据 ───────────
  const dauData = createMemo(() => dau() ?? []);
  const recData = createMemo(() => records() ?? []);
  const pendingSugg = createMemo(() => suggestions() ?? []);
  const wk = createMemo(() => workersResp()?.workers ?? []);

  const isErr = (raw: string | null) => raw != null && /error|fail|down/i.test(raw);
  const isOk = (raw: string | null) => raw != null && /^(ok|success|healthy)$/i.test(raw);
  const wkOk = createMemo(() => wk().filter((w) => isOk(w.lastOutcome)).length);
  const wkErr = createMemo(() => wk().filter((w) => isErr(w.lastOutcome)).length);
  const failed = createMemo(() => wk().find((w) => isErr(w.lastOutcome)) ?? null);

  const dauStats = createMemo(() => {
    const data = dauData();
    if (data.length === 0) return { avg: 0, peak: 0 };
    return {
      avg: Math.round(data.reduce((a, d) => a + d.count, 0) / data.length),
      peak: Math.max(0, ...data.map((d) => d.count)),
    };
  });

  // 算法分布：把 N 天 timeseries 按 algorithm 聚合 callCount + top 6
  const algoDist = createMemo(() => {
    const points = algoSeries() ?? [];
    const agg = new Map<string, number>();
    for (const p of points) agg.set(p.algorithm, (agg.get(p.algorithm) ?? 0) + p.callCount);
    const total = Array.from(agg.values()).reduce((a, b) => a + b, 0);
    const rows = Array.from(agg.entries())
      .map(([algorithm, count]) => ({ algorithm, count }))
      .sort((a, b) => b.count - a.count)
      .slice(0, 6);
    return { rows, total, algoCount: agg.size };
  });

  const alertCount = createMemo(() =>
    (monitoring() ?? []).filter((e) => /error|warn|fail|denied|critical/i.test(e.eventType)).length,
  );

  // KPI 环比
  const recordsDelta = createMemo(() => {
    const pct = overview()?.summary.recordCountDeltaPct;
    return pct == null ? null : pct * 100;
  });
  const accuracyDelta = createMemo(() => {
    const pt = overview()?.summary.accuracyDeltaPt;
    return pt == null ? null : pt * 100;
  });

  const sparkRegistered = createMemo(() => dauData().map((d) => d.registered));
  const sparkActive = createMemo(() => dauData().map((d) => d.count));
  const sparkRecords = createMemo(() => recData().map((d) => d.total));
  const sparkAccuracy = createMemo(() => recData().map((d) => (d.total > 0 ? (d.correct / d.total) * 100 : 0)));

  const heatMax = createMemo(() => {
    const m = hourly()?.matrix ?? [];
    return Math.max(1, ...m.flat());
  });

  // updateInfo banner / version badge
  const hasUpdate = createMemo(() => updateInfo()?.hasUpdate ?? false);

  return (
    <div>
      {/* Page head */}
      <div style={sx({ display: 'flex', flexWrap: 'wrap', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16, marginBottom: 22 })}>
        <div style={sx({ minWidth: 0 })}>
          <h1 class="page-title">全局概览</h1>
          <p class="muted" style={sx({ margin: '8px 0 0', fontSize: 13.5, maxWidth: 680, lineHeight: 1.6 })}>
            注册 · 活跃 · 答题 · 系统健康一屏掌握。所有数据来自后端实时聚合。
          </p>
        </div>
        <div style={sx({ display: 'flex', gap: 8, flexWrap: 'wrap' })}>
          <Seg options={DAYS_OPTS} value={daysStr()} onChange={setDaysStr} />
          <Btn variant="secondary" icon="refresh" onClick={reloadAll}>刷新</Btn>
          <Btn variant="secondary" icon="download" onClick={exportCsv}>导出</Btn>
        </div>
      </div>

      {/* 新版本横幅 */}
      <Show when={hasUpdate()}>
        <div
          class="card fade-up"
          style={sx({
            display: 'flex', alignItems: 'center', gap: 12, padding: '12px 16px', marginBottom: 16,
            borderColor: 'color-mix(in oklch, var(--accent) 40%, var(--border))',
            background: 'color-mix(in oklch, var(--accent) 8%, transparent)',
          })}
        >
          <span style={sx({ display: 'grid', placeItems: 'center', width: 30, height: 30, borderRadius: 9, flex: 'none', background: 'color-mix(in oklch, var(--accent) 16%, transparent)', color: 'var(--accent)' })}>
            <Icon name="update" size={16} />
          </span>
          <div style={sx({ flex: 1, minWidth: 0 })}>
            <div style={sx({ fontWeight: 600, fontSize: 13.5 })}>新版本 {updateInfo()?.latestVersion ?? ''} 可用</div>
            <div class="muted-3" style={sx({ fontSize: 12 })}>当前 {updateInfo()?.currentVersion ?? ''} · 查看发行说明并升级</div>
          </div>
          <Btn variant="primary" size="sm" icon="chevR" onClick={() => navigate('/admin/updates')}>前往升级</Btn>
        </div>
      </Show>

      {/* KPI */}
      <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: 16, marginBottom: 16 })}>
        <Show when={stats()} fallback={<Skel h={116} />}>
          {(s) => (
            <StatCard
              tone="accent" label="注册用户" icon="users" value={fmtNum(s().users)}
              delta={s().trend?.users?.value ?? null} deltaLabel="较上周期" spark={sparkRegistered()}
            />
          )}
        </Show>
        <Show when={eng()} fallback={<Skel h={116} />}>
          {(e) => (
            <StatCard
              tone="success" label="今日活跃" icon="zap" value={fmtNum(e().activeToday)}
              delta={e().trend?.activeToday?.value ?? null} deltaLabel="较昨日" spark={sparkActive()}
            />
          )}
        </Show>
        <Show when={overview()} fallback={<Skel h={116} />}>
          {(o) => (
            <StatCard
              tone="info" label={`${days()} 天答题数`} icon="check" value={fmtNum(o().summary.recordCount)}
              delta={recordsDelta()} deltaLabel="较上周期" spark={sparkRecords()}
            />
          )}
        </Show>
        <Show when={overview()} fallback={<Skel h={116} />}>
          {(o) => (
            <StatCard
              tone="warning" label={`${days()} 天正确率`} icon="chart"
              value={o().summary.accuracy != null ? (o().summary.accuracy! * 100).toFixed(1) : '—'}
              unit={o().summary.accuracy != null ? '%' : undefined}
              delta={accuracyDelta()}
              deltaLabel={o().summary.prevAccuracy != null ? `上周 ${(o().summary.prevAccuracy! * 100).toFixed(1)}%` : '较上周期'}
              spark={sparkAccuracy()}
            />
          )}
        </Show>
      </div>

      {/* Row A：用户活跃趋势 + 算法分布 */}
      <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,2fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })}>
        <Panel
          title="用户活跃趋势"
          right={
            <div style={sx({ display: 'flex', gap: 18 })}>
              <MiniStat label="平均日活" value={fmtNum(dauStats().avg)} tone="accent" />
              <MiniStat label="峰值日活" value={fmtNum(dauStats().peak)} tone="success" />
            </div>
          }
        >
          <Show when={dau.error} fallback={
            <Show when={dau()} fallback={<Loading h={280} />}>
              <LineChart
                height={280}
                labels={dauData().map((d) => d.date.slice(5))}
                series={[
                  { name: '日活跃', data: dauData().map((d) => d.count), color: 'var(--accent)' },
                  { name: '新注册', data: dauData().map((d) => d.registered), color: 'var(--success)' },
                ]}
              />
            </Show>
          }>
            <Empty title="加载失败" desc="无法获取活跃数据" icon="alert" />
          </Show>
        </Panel>

        <Panel
          title="AMAS 算法分布" sub={`${days()} 天 · ${fmtNum(algoDist().total)} 次决策`}
          right={<a onClick={() => navigate('/admin/amas-metrics')} style={sx({ fontSize: 12, color: 'var(--text-3)', cursor: 'pointer' })}>详情 →</a>}
        >
          <Show when={algoSeries.state !== 'pending'} fallback={<Loading h={220} />}>
            <Show when={algoDist().rows.length > 0} fallback={<Empty title="暂无数据" desc="决策事件未上报" icon="chart" />}>
              <Donut
                size={150} thickness={22}
                centerValue={algoDist().algoCount} centerLabel="算法"
                data={algoDist().rows.map((a, i) => ({ label: a.algorithm, value: a.count, color: ALGO_COLORS[i % ALGO_COLORS.length] }))}
              />
            </Show>
          </Show>
        </Panel>
      </div>

      {/* Row B：学习产出 + 时段热图 */}
      <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,2fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })}>
        <Panel
          title="学习产出"
          right={
            <Show when={overview()}>
              {(o) => (
                <div style={sx({ display: 'flex', gap: 18 })}>
                  <MiniStat label="累计时长" value={fmtDur(o().summary.totalDurationSecs)} tone="info" />
                  <MiniStat label="新增单词" value={fmtNum(o().summary.newWords)} tone="warning" />
                </div>
              )}
            </Show>
          }
        >
          <Show when={records.error} fallback={
            <Show when={records()} fallback={<Loading h={260} />}>
              <LineChart
                height={260} area={false}
                labels={recData().map((d) => d.date.slice(5))}
                series={[{ name: '答题数', data: recData().map((d) => d.total), color: 'var(--accent)' }]}
              />
            </Show>
          }>
            <Empty title="加载失败" desc="无法获取答题记录" icon="alert" />
          </Show>
        </Panel>

        <Panel title="用户学习时段" sub="24h × 7d 热图">
          <Show when={hourly.state !== 'pending'} fallback={<Loading h={220} />}>
            <Show when={hourly()} fallback={<Empty title="暂无数据" desc="未生成学习时段数据" icon="clock" />}>
              {(h) => (
                <Heatmap
                  rows={7} cols={24} cellH={15} gap={2}
                  rowLabels={DOW}
                  colLabels={Array.from({ length: 24 }, (_, i) => String(i))}
                  values={h().matrix}
                  colorFor={(v) => `color-mix(in oklch, var(--accent) ${Math.round((v / heatMax()) * 100)}%, transparent)`}
                  fmtCell={(r, c, v) => `${DOW[r]} ${c}:00 · ${v} 次`}
                />
              )}
            </Show>
          </Show>
        </Panel>
      </div>

      {/* Row C：系统状态 + Worker 心跳 */}
      <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) minmax(0,1.4fr)', gap: 16, marginBottom: 16 })}>
        <Panel title="系统状态">
          <Show when={health()} fallback={<Loading h={240} />}>
            {(h) => (
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
                <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 14 })}>
                  <KV label="状态">
                    <span style={sx({
                      display: 'inline-flex', gap: 6, alignItems: 'center', fontWeight: 600,
                      color: h().status === 'healthy' ? 'var(--success)' : h().status === 'degraded' ? 'var(--warning)' : 'var(--error)',
                    })}>
                      <Show when={h().status === 'healthy'}><span class="live-dot" /></Show>
                      {h().status === 'healthy' ? '运行正常' : h().status === 'degraded' ? '性能降级' : '服务异常'}
                    </span>
                  </KV>
                  <KV label="数据库" value={fmtBytes(h().dbSizeBytes)} mono />
                  <KV label="运行时间" value={fmtDur(h().uptimeSecs)} mono />
                  <KV label="版本">
                    <span class="mono" style={sx({ fontWeight: 600 })}>{h().version}</span>
                    <Show when={hasUpdate()}>
                      {' '}<Badge variant="accent">新版 {updateInfo()?.latestVersion ?? ''}</Badge>
                    </Show>
                  </KV>
                </div>
                <Show when={h().resources}>
                  {(res) => (
                    <div style={sx({ borderTop: '1px solid var(--hairline)', paddingTop: 12, display: 'flex', flexDirection: 'column', gap: 9 })}>
                      <Show when={res().cpuPct != null}>
                        <ResBar label="CPU 使用" value={`${res().cpuPct!.toFixed(1)}%`} pct={Math.min(res().cpuPct!, 100)} />
                      </Show>
                      <Show when={res().memoryRssBytes != null}>
                        <ResBar
                          label="内存 RSS" value={fmtBytes(res().memoryRssBytes)}
                          pct={res().memoryTotalBytes ? (res().memoryRssBytes! / res().memoryTotalBytes!) * 100 : null}
                        />
                      </Show>
                      <Show when={res().pool}>
                        {(p) => (
                          <ResBar
                            label="DB 连接池" value={`${p().connections} / ${p().max}`}
                            pct={p().max > 0 ? (p().connections / p().max) * 100 : null}
                            tone={p().max > 0 && p().connections / p().max > 0.8 ? 'warning' : 'success'}
                          />
                        )}
                      </Show>
                      <Show when={res().diskTotalBytes != null && res().diskFreeBytes != null}>
                        <ResBar
                          label="磁盘"
                          value={`${fmtBytes(res().diskTotalBytes! - res().diskFreeBytes!)} / ${fmtBytes(res().diskTotalBytes!)}`}
                          pct={res().diskTotalBytes! > 0 ? ((res().diskTotalBytes! - res().diskFreeBytes!) / res().diskTotalBytes!) * 100 : null}
                        />
                      </Show>
                    </div>
                  )}
                </Show>
              </div>
            )}
          </Show>
        </Panel>

        <Panel
          title="Worker 心跳" sub={`${wk().length} 个 tokio worker`}
          right={<a onClick={() => navigate('/admin/monitoring')} style={sx({ fontSize: 12, color: 'var(--text-3)', cursor: 'pointer' })}>完整监控 →</a>}
        >
          <Show when={workersResp.state !== 'pending'} fallback={<Loading h={220} />}>
            <Show when={wk().length > 0} fallback={<Empty title="暂无 worker" desc="后端未注册 worker 心跳" icon="cpu" />}>
              <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(94px, 1fr))', gap: 7 })}>
                <For each={wk()}>
                  {(w) => {
                    const col = isOk(w.lastOutcome) ? 'var(--success)' : isErr(w.lastOutcome) ? 'var(--error)' : 'var(--text-3)';
                    return (
                      <div
                        title={`${w.lastError || w.lastOutcome || 'unknown'} · ${fmtAgo(w.lastRunAt)}`}
                        style={sx({ padding: '8px 9px', borderRadius: 9, border: '1px solid var(--hairline)', background: 'var(--surface-sunken)', display: 'flex', flexDirection: 'column', gap: 5 })}
                      >
                        <span style={sx({ display: 'flex', alignItems: 'center', gap: 5 })}>
                          <span style={sx({ width: 7, height: 7, borderRadius: 50, background: col, flex: 'none' })} />
                          <span style={sx({ fontSize: 10.5, fontWeight: 600, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{w.workerName}</span>
                        </span>
                        <span class="mono muted-3" style={sx({ fontSize: 9.5 })}>{w.lastDurationMs ?? '—'}ms</span>
                      </div>
                    );
                  }}
                </For>
              </div>
              <div style={sx({ marginTop: 12, paddingTop: 11, borderTop: '1px solid var(--hairline)', display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: 12 })}>
                <span class="muted">
                  <b style={sx({ color: 'var(--success)' })}>{wkOk()}</b> 健康 ·{' '}
                  <b style={sx({ color: 'var(--error)' })}>{wkErr()}</b> 异常
                  <Show when={failed()}>{(f) => <span class="muted-3">（{f().workerName}）</span>}</Show>
                </span>
                <Show when={wkErr() > 0}>
                  <a onClick={() => navigate('/admin/monitoring')} style={sx({ color: 'var(--warning)', cursor: 'pointer' })}>查看 →</a>
                </Show>
              </div>
            </Show>
          </Show>
        </Panel>
      </div>

      {/* Row D：LLM 待审建议 + 待处理事项 */}
      <div class="grid-collapse" style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.4fr) minmax(0,1fr)', gap: 16 })}>
        <Panel
          title="LLM 调参顾问 · 待审建议" sub={`pending ${pendingSugg().length} 条`}
          right={<a onClick={() => navigate('/admin/amas-advisor')} style={sx({ fontSize: 12, color: 'var(--text-3)', cursor: 'pointer' })}>前往顾问 →</a>}
        >
          <Show when={suggestions.state !== 'pending'} fallback={<Loading h={160} />}>
            <Show when={pendingSugg().length > 0} fallback={<Empty title="暂无待审建议" icon="bulb" />}>
              <div style={sx({ display: 'flex', flexDirection: 'column' })}>
                <For each={pendingSugg().slice(0, 3)}>
                  {(s) => (
                    <div style={sx({ padding: '12px 0', borderBottom: '1px solid var(--hairline)' })}>
                      <div style={sx({ display: 'flex', justifyContent: 'space-between', gap: 12 })}>
                        <p style={sx({ margin: 0, fontSize: 13, fontWeight: 500, lineHeight: 1.5 })}>{s.rationale}</p>
                        <Badge variant="warning">待审</Badge>
                      </div>
                      <div class="muted-3" style={sx({ display: 'flex', gap: 10, marginTop: 7, fontSize: 11.5, flexWrap: 'wrap' })}>
                        <span class="mono">patch {Object.keys(s.patchJson).length} 项</span>
                        <Show when={s.confidence != null}>
                          <span>置信度 <b class="mono">{(s.confidence! * 100).toFixed(0)}%</b></span>
                        </Show>
                        <Show when={s.costUsd != null}>
                          <span>成本 <b class="mono">${s.costUsd!.toFixed(4)}</b></span>
                        </Show>
                        <a onClick={() => navigate('/admin/amas-advisor')} style={sx({ marginLeft: 'auto', color: 'var(--accent)', cursor: 'pointer' })}>查看 diff →</a>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Show>
        </Panel>

        <Panel title="待处理事项" sub="需要管理员关注">
          <div style={sx({ display: 'flex', flexDirection: 'column' })}>
            <Show when={hasUpdate()}>
              <TodoRow
                tone="accent" icon="update" title={`新版本 ${updateInfo()?.latestVersion ?? ''} 可用`}
                desc="查看发行说明并升级" onClick={() => navigate('/admin/updates')}
              />
            </Show>
            <Show when={(openFeedback()?.total ?? 0) > 0}>
              <TodoRow
                tone="warning" icon="inbox" title={`${openFeedback()!.total} 条未处理反馈`}
                desc="含 bug 报告与功能请求" onClick={() => navigate('/admin/feedback?status=open')}
              />
            </Show>
            <Show when={failed()}>
              {(f) => (
                <TodoRow
                  tone="error" icon="alert" title={`Worker ${f().workerName} 执行异常`}
                  desc={f().lastError ?? '最近一次执行返回 error，请前往监控排查'} onClick={() => navigate('/admin/monitoring')}
                />
              )}
            </Show>
            <Show when={alertCount() > 0}>
              <TodoRow
                tone="warning" icon="monitor" title={`${alertCount()} 条监控告警`}
                desc="最近 20 条事件含 error / warn" onClick={() => navigate('/admin/monitoring')}
              />
            </Show>
            <Show when={pendingSugg().length > 0}>
              <TodoRow
                tone="accent" icon="bulb" title={`LLM 顾问 · ${pendingSugg().length} 条建议待审`}
                desc="approve 后进入灰度发布" onClick={() => navigate('/admin/amas-advisor')}
              />
            </Show>
            <Show when={
              !hasUpdate() && (openFeedback()?.total ?? 0) === 0 && !failed()
              && alertCount() === 0 && pendingSugg().length === 0
              && !openFeedback.loading && !suggestions.loading
            }>
              <div style={sx({ display: 'flex', alignItems: 'center', gap: 8, padding: '16px 4px', fontSize: 13, color: 'var(--text-3)' })}>
                <span class="live-dot" />当前无待办，系统运行平稳。
              </div>
            </Show>
          </div>
        </Panel>
      </div>
    </div>
  );
}
