import { createResource, createSignal, createEffect, For, Show, type JSX } from 'solid-js';
import {
  PageHead, Seg, Panel, StatCard, Badge, Icon, Field, Loading, Skel,
  Donut, LineChart, BarChart, Scatter, Heatmap,
  fmtNum, fmtAgo, sx,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import type {
  AmasMetricsKpi, AmasAlgorithmShare, AmasStageDistribution, AmasEloScatter,
  AmasMdmHeatmap, AmasFatigueTimeseries, AmasDecisionHistogram, AmasUserStateDistribution,
  AmasUserStateHistogram, AmasStateTransitions, AmasLearningStyleClusters, AmasAnomalyFeed,
  AmasConfigVersionRow, AmasVersionSliceExt,
} from '@/api/admin';

/* ============================================================
   AMAS — 指标看板（对齐设计稿 pages-amas.jsx · amas-metrics）
   决策 / 命中率 / ELO / MDM / 疲劳 / 阶段 / 异常 / 版本对比
   ============================================================ */

const ALGO_COLORS = ['#7c5cff', '#4f7dfb', '#18a558', '#d99411', '#e5484d', '#0ea5b7', '#a855f7', '#64748b'];
const STAGE_COLOR: Record<string, string> = { cold: '#d99411', transition: '#4f7dfb', stable: '#18a558' };
const STAGE_LABEL: Record<string, string> = { cold: '冷启动', transition: '过渡', stable: '稳定' };

const DAY_OPTS = [
  { value: '7', label: '7 天' },
  { value: '14', label: '14 天' },
  { value: '30', label: '30 天' },
];

type CmpExt = { a: AmasVersionSliceExt; b: AmasVersionSliceExt };

export default function AmasMetricsPage() {
  const [days, setDays] = createSignal(7);

  // 各看板独立窗口加载（match prototype useAsync 粒度）
  const [kpi] = createResource(days, (d) => adminApi.amasMetricsKpi(d));
  const [algo] = createResource(days, (d) => adminApi.amasAlgorithmDistribution(d));
  const [stage] = createResource(() => adminApi.amasStageDistribution());
  const [elo] = createResource(() => adminApi.amasEloScatter(400));
  const [mdm] = createResource(days, (d) => adminApi.amasMdmHeatmap(d));
  const [fatigue] = createResource(days, (d) => adminApi.amasFatigueTimeseries(d));
  const [hist] = createResource(days, (d) => adminApi.amasDecisionHistogram(d));
  const [ustate] = createResource(() => adminApi.amasUserStateDistribution());
  const [transitions] = createResource(() => adminApi.amasStateTransitions(24));
  const [clusters] = createResource(() => adminApi.amasLearningClusters());
  const [anomalies] = createResource(() => adminApi.amasAnomalyFeed());
  const [versions] = createResource(() => adminApi.amasListVersions());

  const [cmpA, setCmpA] = createSignal('');
  const [cmpB, setCmpB] = createSignal('');
  // 默认选中最近两个版本（A=次新, B=最新）
  createEffect(() => {
    const list = versions();
    if (list && list.length >= 2 && !cmpA()) {
      setCmpA(list[1].versionHash);
      setCmpB(list[0].versionHash);
    }
  });
  const [cmp] = createResource(
    () => [cmpA(), cmpB()] as [string, string],
    ([a, b]): Promise<CmpExt | null> => (a && b ? adminApi.amasCompareVersionsExt(a, b) : Promise.resolve(null)),
  );

  // 导出当前窗口算法时序为 CSV
  const exportCsv = async () => {
    const rows = await adminApi.amasMetricsTimeseries(days());
    const head = 'date,algorithm,callCount,avgLatencyUs,errorCount';
    const body = rows
      .map((p) => [p.date, p.algorithm, p.callCount, p.avgLatencyUs, p.errorCount].join(','))
      .join('\n');
    const blob = new Blob([`${head}\n${body}`], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `amas-metrics-${days()}d.csv`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div class="fade-up">
      <PageHead
        title="AMAS 指标"
        desc="自适应记忆算法系统的决策、命中率、疲劳、ELO 与异常的实时聚合看板。"
        right={
          <>
            <Seg options={DAY_OPTS} value={String(days())} onChange={(v) => setDays(+v)} />
            <button type="button" class="btn btn-secondary btn-sm" onClick={exportCsv}>
              <Icon name="download" size={14} /> 导出
            </button>
          </>
        }
      />

      {/* KPI */}
      <div
        style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(210px,1fr))', gap: 16, marginBottom: 16 })}
      >
        <Show
          when={kpi()}
          fallback={
            <>
              <Skel h={116} /><Skel h={116} /><Skel h={116} /><Skel h={116} />
            </>
          }
        >
          {(k) => (
            <>
              <StatCard tone="accent" label="决策总数" icon="zap" value={fmtNum(k().decisionTotal)} deltaLabel={days() + ' 天累计'} />
              <StatCard tone="success" label="命中率" icon="check" value={(k().hitRate * 100).toFixed(1)} unit="%" deltaLabel={fmtNum(k().hitCount) + ' 命中'} />
              <StatCard tone="info" label="平均决策耗时" icon="clock" value={k().avgLatencyMs.toFixed(2)} unit="ms" deltaLabel="单次决策" />
              <StatCard tone="warning" label="疲劳触发率" icon="alert" value={(k().fatigueTriggerRate * 100).toFixed(2)} unit="%" deltaLabel={'阈值 ' + k().fatigueThreshold} />
            </>
          )}
        </Show>
      </div>

      {/* 算法分布 + 阶段分布 */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) minmax(0,1.4fr)', gap: 16, marginBottom: 16 })}
      >
        <Panel title="算法决策分布" sub="各算法占比">
          <Show when={algo()} fallback={<Loading />}>
            {(a) => (
              <Donut
                size={160}
                thickness={24}
                centerValue={a().length}
                centerLabel="算法"
                data={a().map((row: AmasAlgorithmShare, i) => ({
                  label: row.algorithm,
                  value: row.count,
                  color: ALGO_COLORS[i % ALGO_COLORS.length],
                }))}
              />
            )}
          </Show>
        </Panel>
        <Panel title="用户阶段分布" sub="cold · transition · stable">
          <Show when={stage()} fallback={<Loading />}>
            {(s) => (
              <div>
                <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 10, marginBottom: 14 })}>
                  <For each={s().stages}>
                    {(st) => (
                      <div style={sx({ padding: 12, borderRadius: 11, background: 'var(--surface-sunken)', borderTop: '3px solid ' + (STAGE_COLOR[st.stage] ?? 'var(--border)') })}>
                        <div style={sx({ fontSize: 12, fontWeight: 600, color: STAGE_COLOR[st.stage] ?? 'var(--text)' })}>{STAGE_LABEL[st.stage] ?? st.stage}</div>
                        <div class="tnum" style={sx({ fontSize: 22, fontWeight: 800, marginTop: 3 })}>{fmtNum(st.users)}</div>
                        <div class="muted-3" style={sx({ fontSize: 10.5, marginTop: 4 })}>{(st.pct * 100).toFixed(0)}% · 留存 {(st.retention7d * 100).toFixed(0)}%</div>
                        <div class="muted-3 mono" style={sx({ fontSize: 10, marginTop: 2 })}>主路由 {st.mainRoute}</div>
                      </div>
                    )}
                  </For>
                </div>
                <LineChart
                  height={170}
                  labels={s().trend.map((t) => t.date.slice(5))}
                  series={[
                    { name: '冷启动', data: s().trend.map((t) => t.cold), color: STAGE_COLOR.cold },
                    { name: '过渡', data: s().trend.map((t) => t.transition), color: STAGE_COLOR.transition },
                    { name: '稳定', data: s().trend.map((t) => t.stable), color: STAGE_COLOR.stable },
                  ]}
                />
              </div>
            )}
          </Show>
        </Panel>
      </div>

      {/* ELO 散点 + MDM 热图 */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16, marginBottom: 16 })}
      >
        <Panel title="ELO × 决策数散点" sub={elo() ? '均值 ELO ' + elo()!.meanElo + ' · ' + elo()!.total + ' 用户' : ''}>
          <Show when={elo()} fallback={<Loading />}>
            {(e) => (
              <Scatter
                height={260}
                xLabel="ELO 评分"
                points={e().points.map((p) => ({ x: p.elo, y: p.decisions, r: 3, dz: p.deltaElo }))}
                colorFor={(p) => ((p.dz as number) >= 0 ? 'var(--success)' : 'var(--error)')}
              />
            )}
          </Show>
        </Panel>
        <Panel title="MDM 遗忘概率热图" sub="天 × 难度段">
          <Show when={mdm()} fallback={<Loading />}>
            {(m) => (
              <Heatmap
                rows={m().cells.length}
                cols={m().bandCount}
                cellH={20}
                gap={2}
                rowLabels={m().days.map((d) => d.slice(5))}
                colLabels={Array.from({ length: m().bandCount }, (_, i) => String(i + 1))}
                values={m().cells}
                colorFor={(v) => `color-mix(in oklch, var(--error) ${Math.round(v * 100)}%, transparent)`}
                fmtCell={(_r, _c, v) => '遗忘概率 ' + (v >= 0 ? v.toFixed(2) : '—')}
              />
            )}
          </Show>
        </Panel>
      </div>

      {/* 疲劳时序 + 决策直方图 */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.4fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })}
      >
        <Panel title="疲劳信号时序" sub={fatigue() ? '总触发 ' + fmtNum(fatigue()!.totalTriggers) + ' · 阈值 ' + fatigue()!.threshold : ''}>
          <Show when={fatigue()} fallback={<Loading />}>
            {(f) => (
              <LineChart
                height={230}
                yMax={1}
                fmtY={(v) => v.toFixed(1)}
                labels={f().points.map((p) => p.date.slice(5))}
                series={[
                  { name: '平均疲劳', data: f().points.map((p) => p.avgFatigue), color: 'var(--warning)' },
                  { name: '峰值疲劳', data: f().points.map((p) => p.peakFatigue), color: 'var(--error)' },
                ]}
              />
            )}
          </Show>
        </Panel>
        <Panel title="每用户决策数分布" sub={hist() ? 'P50 ' + hist()!.p50 + ' · P95 ' + hist()!.p95 : ''}>
          <Show when={hist()} fallback={<Loading />}>
            {(h) => <BarChart height={230} data={h().buckets.map((b) => ({ label: b.label, value: b.count }))} />}
          </Show>
        </Panel>
      </div>

      {/* 用户状态分布（4 直方图） */}
      <Panel title="用户状态分布" sub="注意力 · 疲劳 · 动机 · 自信（直方图）" style={{ 'margin-bottom': '16px' }}>
        <Show when={ustate()} fallback={<Loading />}>
          {(u) => (
            <div style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px,1fr))', gap: 18 })}>
              <For
                each={
                  [
                    ['attention', '注意力', 'var(--accent)', u().attention],
                    ['fatigue', '疲劳', 'var(--warning)', u().fatigue],
                    ['motivation', '动机', 'var(--success)', u().motivation],
                    ['confidence', '自信', 'var(--info)', u().confidence],
                  ] as [string, string, string, AmasUserStateHistogram][]
                }
              >
                {([key, label, col, h]) => {
                  const max = Math.max(1, ...h.bins);
                  return (
                    <div>
                      <div style={sx({ display: 'flex', justifyContent: 'space-between', marginBottom: 8 })}>
                        <span style={sx({ fontWeight: 600, fontSize: 13 })}>{label}</span>
                        <span class="mono muted-3" style={sx({ fontSize: 11 })}>μ {h.mean.toFixed(2)}</span>
                      </div>
                      <div style={sx({ display: 'flex', alignItems: 'flex-end', gap: 2, height: 80 })}>
                        <For each={h.bins}>
                          {(b, i) => (
                            <div
                              title={String(b)}
                              style={sx({
                                flex: 1,
                                height: (b / max * 100) + '%',
                                background: col,
                                opacity: 0.45 + (i() / h.bins.length) * 0.5,
                                borderRadius: '2px 2px 0 0',
                              })}
                            />
                          )}
                        </For>
                      </div>
                      <div class="muted-3 mono" style={sx({ fontSize: 10, marginTop: 4, textAlign: 'center' })} data-key={key}>n={fmtNum(h.sampleSize)}</div>
                    </div>
                  );
                }}
              </For>
            </div>
          )}
        </Show>
      </Panel>

      {/* 学习风格聚类 + 阶段流转 */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.4fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })}
      >
        <Panel title="学习风格聚类" sub={clusters() ? `k-means · k=${clusters()!.k}` : 'k-means'}>
          <Show when={clusters()} fallback={<Loading />}>
            {(c) => (
              <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
                <For each={c().clusters}>
                  {(cl, i) => (
                    <div style={sx({ display: 'grid', gridTemplateColumns: '110px 1fr auto', gap: 12, alignItems: 'center' })}>
                      <div>
                        <div style={sx({ fontWeight: 600, fontSize: 13 })}>{cl.label}</div>
                        <div class="muted-3" style={sx({ fontSize: 10.5 })}>{fmtNum(cl.count)} 人</div>
                      </div>
                      <div class="bar" style={sx({ height: 8 })}>
                        <i style={sx({ width: (cl.pct * 100) + '%', background: ALGO_COLORS[i() % ALGO_COLORS.length] })} />
                      </div>
                      <div class="muted mono" style={sx({ fontSize: 11, textAlign: 'right' })}>{Math.round(cl.avgResponseMs)}ms · 错误 {(cl.errorRate * 100).toFixed(0)}%</div>
                    </div>
                  )}
                </For>
              </div>
            )}
          </Show>
        </Panel>
        <Panel title="阶段流转" sub="24h 窗口">
          <Show when={transitions()} fallback={<Loading />}>
            {(t) => {
              const list = t().transitions;
              const max = Math.max(1, ...list.map((x) => x.count));
              return (
                <Show when={list.length} fallback={<div class="muted-3" style={sx({ fontSize: 12, padding: 8 })}>窗口内无阶段流转</div>}>
                  <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
                    <For each={list}>
                      {(x) => (
                        <div style={sx({ display: 'flex', alignItems: 'center', gap: 9, fontSize: 12.5 })}>
                          <Badge style={sx({ background: `color-mix(in oklch, ${STAGE_COLOR[x.from] ?? 'var(--text-3)'} 16%, transparent)`, color: STAGE_COLOR[x.from] ?? 'var(--text-2)' })}>{STAGE_LABEL[x.from] ?? x.from}</Badge>
                          <Icon name="chevR" size={13} />
                          <Badge style={sx({ background: `color-mix(in oklch, ${STAGE_COLOR[x.to] ?? 'var(--text-3)'} 16%, transparent)`, color: STAGE_COLOR[x.to] ?? 'var(--text-2)' })}>{STAGE_LABEL[x.to] ?? x.to}</Badge>
                          <div class="bar" style={sx({ height: 6, flex: 1 })}><i style={sx({ width: (x.count / max * 100) + '%' })} /></div>
                          <span class="mono" style={sx({ width: 36, textAlign: 'right' })}>{x.count}</span>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              );
            }}
          </Show>
        </Panel>
      </div>

      {/* 异常检测 */}
      <Panel
        title="异常检测"
        sub={anomalies() ? '不变量通过率 ' + (anomalies()!.summary.passRate * 100).toFixed(1) + '% (' + anomalies()!.summary.invariantPass + '/' + anomalies()!.summary.invariantTotal + ')' : ''}
        style={{ 'margin-bottom': '16px' }}
        right={
          <Show when={anomalies()}>
            {(an) => (
              <div style={sx({ display: 'flex', gap: 7 })}>
                <Badge variant="error">{an().summary.error} error</Badge>
                <Badge variant="warning">{an().summary.warn} warn</Badge>
                <Badge variant="info">{an().summary.info} info</Badge>
              </div>
            )}
          </Show>
        }
      >
        <Show when={anomalies()} fallback={<Loading />}>
          {(an) => (
            <Show
              when={an().items.length}
              fallback={<div class="muted-3" style={sx({ fontSize: 12, padding: 8 })}>窗口内未检出异常</div>}
            >
              <div style={sx({ overflowX: 'auto' })}>
                <table class="tbl">
                  <thead>
                    <tr><th>级别</th><th>代码</th><th>描述</th><th>当前值</th><th>期望区间</th><th>影响用户</th><th>时间</th></tr>
                  </thead>
                  <tbody>
                    <For each={an().items.slice(0, 10)}>
                      {(a) => (
                        <tr>
                          <td><Badge variant={a.severity === 'error' ? 'error' : a.severity === 'warn' ? 'warning' : 'info'} dot>{a.severity}</Badge></td>
                          <td class="mono" style={sx({ fontSize: 11.5 })}>{a.code}</td>
                          <td style={sx({ fontSize: 12.5 })}>{a.title}</td>
                          <td class="mono tnum">{a.value}</td>
                          <td class="mono muted" style={sx({ fontSize: 11.5 })}>{a.expectedRange}</td>
                          <td class="mono">{a.impactedUsers} ({(a.impactPct * 100).toFixed(1)}%)</td>
                          <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtAgo(a.timestamp)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>
          )}
        </Show>
      </Panel>

      {/* 版本对比 */}
      <Panel title="版本对比" sub="A / B 配置版本的指标切片对比">
        <Show when={versions()} fallback={<Loading />}>
          {(vs) => (
            <>
              <div style={sx({ display: 'flex', gap: 12, marginBottom: 16, flexWrap: 'wrap' })}>
                <Field label="版本 A">
                  <select class="select" value={cmpA()} onChange={(e) => setCmpA(e.currentTarget.value)} style={sx({ minWidth: 200 })}>
                    <For each={vs()}>{(v: AmasConfigVersionRow) => <option value={v.versionHash}>{v.versionHash} · {(v.note ?? '').slice(0, 16)}</option>}</For>
                  </select>
                </Field>
                <Field label="版本 B">
                  <select class="select" value={cmpB()} onChange={(e) => setCmpB(e.currentTarget.value)} style={sx({ minWidth: 200 })}>
                    <For each={vs()}>{(v: AmasConfigVersionRow) => <option value={v.versionHash}>{v.versionHash} · {(v.note ?? '').slice(0, 16)}</option>}</For>
                  </select>
                </Field>
              </div>
              <Show when={cmp.state !== 'pending'} fallback={<Loading />}>
                <Show when={cmp()} fallback={<div class="muted-3" style={sx({ fontSize: 12, padding: 8 })}>选择两个版本以对比指标切片</div>}>
                  {(c) => <CompareTable cmp={c()} />}
                </Show>
              </Show>
            </>
          )}
        </Show>
      </Panel>
    </div>
  );
}

/* ---- 版本对比表（lower-is-better 字段单独判定优劣方向） ---- */
const CMP_ROWS: [string, keyof AmasVersionSliceExt, (v: number) => string][] = [
  ['命中率', 'hitRate', (v) => (v * 100).toFixed(1) + '%'],
  ['P95 延迟', 'p95LatencyMs', (v) => v + 'ms'],
  ['疲劳率', 'fatigueRate', (v) => (v * 100).toFixed(2) + '%'],
  ['ensemble 占比', 'ensembleShare', (v) => (v * 100).toFixed(0) + '%'],
  ['平均奖励', 'meanReward', (v) => v.toFixed(3)],
  ['异常率', 'anomalyRate', (v) => (v * 100).toFixed(2) + '%'],
  ['7d 留存', 'retention7d', (v) => (v * 100).toFixed(0) + '%'],
];
const LOWER_BETTER = new Set<keyof AmasVersionSliceExt>(['p95LatencyMs', 'fatigueRate', 'anomalyRate']);

function CompareTable(props: { cmp: CmpExt }): JSX.Element {
  return (
    <div style={sx({ overflowX: 'auto' })}>
      <table class="tbl">
        <thead>
          <tr><th>指标</th><th class="mono">{props.cmp.a.versionHash}</th><th class="mono">{props.cmp.b.versionHash}</th><th>变化</th></tr>
        </thead>
        <tbody>
          <For each={CMP_ROWS}>
            {([label, key, fmt]) => {
              const a = props.cmp.a[key] as number;
              const b = props.cmp.b[key] as number;
              const diff = b - a;
              const better = LOWER_BETTER.has(key) ? diff < 0 : diff > 0;
              const flat = Math.abs(diff) < 1e-6;
              return (
                <tr>
                  <td style={sx({ fontWeight: 600, fontSize: 12.5 })}>{label}</td>
                  <td class="mono">{fmt(a)}</td>
                  <td class="mono">{fmt(b)}</td>
                  <td>
                    <span style={sx({ color: flat ? 'var(--text-3)' : better ? 'var(--success)' : 'var(--error)', fontWeight: 600, fontSize: 12 })}>
                      {diff >= 0 ? '▲' : '▼'} {fmt(Math.abs(diff))}
                    </span>
                  </td>
                </tr>
              );
            }}
          </For>
        </tbody>
      </table>
    </div>
  );
}
