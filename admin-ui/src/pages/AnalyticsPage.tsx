import { createMemo, createResource, createSignal, Show, For, type JSX } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import {
  Btn, Seg, Panel, PageHead, StatCard, Loading, Skel, Donut, BarChart, Heatmap, Modal,
  fmtNum, fmtDur, sx, toast,
  type DonutDatum,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import type {
  AnalyticsKpiSummary, AnalyticsKpiPctDelta, AnalyticsKpiPtDelta,
  AnalyticsFunnel, AnalyticsRetentionMatrix, AnalyticsQuestionDistribution,
  AnalyticsWordFrequency, AnalyticsWordFrequencyRow, AnalyticsInsights, AnalyticsInsightItem,
  WordbookRankRow,
} from '@/api/admin';

/* ── 常量 ───────────────────────────────────────────────────────────── */

const DAYS_ALLOWED = [7, 14, 30, 90] as const;
const DAY_OPTIONS = DAYS_ALLOWED.map((d) => ({ value: String(d), label: `${d} 天` }));

type WordSort = 'count' | 'accuracy' | 'elo' | 'mastery';

/** 题型 donut 配色（按序号轮换） */
const QTYPE_COLORS = ['var(--accent)', 'var(--success)', 'var(--info)', 'var(--warning)', 'var(--error)'];

/** 洞察左侧色条 / 标题颜色 */
const INSIGHT_COLOR: Record<AnalyticsInsightItem['tone'], string> = {
  success: 'var(--success)',
  warning: 'var(--warning)',
  info: 'var(--info)',
  accent: 'var(--accent)',
};

/** 后端 hourly matrix 行序按 strftime('%w'):0=周日。展示顺序对齐设计稿 周一→周日。 */
const WEEKDAY_CN = ['日', '一', '二', '三', '四', '五', '六'];
const DOW_ORDER = [1, 2, 3, 4, 5, 6, 0];
const HOUR_LABELS = Array.from({ length: 24 }, (_, h) => String(h).padStart(2, '0'));

/* ── 辅助 ───────────────────────────────────────────────────────────── */

/** KPI 百分比 delta → StatCard 期望的 *100 数值（null 时不显示徽标） */
function pctDelta(d: AnalyticsKpiPctDelta): number | null {
  return d.deltaPct == null ? null : d.deltaPct * 100;
}
/** KPI 百分点 delta → *100（pt 已是 0-1 小数差，×100 得百分点数值） */
function ptDelta(d: AnalyticsKpiPtDelta): number | null {
  return d.deltaPt == null ? null : d.deltaPt * 100;
}

/** 漏斗单步条形配色 */
function funnelColor(tone: AnalyticsFunnel['steps'][number]['tone']): string {
  return tone === 'good' ? 'var(--success)' : tone === 'warn' ? 'var(--warning)' : 'var(--accent)';
}

/** cohort 留存格背景：按留存率深浅映射 accent */
function cohortBg(v: number): string {
  return `color-mix(in oklch, var(--accent) ${Math.round(v * 90)}%, transparent)`;
}

/** hourly 热图配色（按相对峰值 0-1） */
function heatColor(v: number): string {
  if (v <= 0) return 'var(--surface-sunken)';
  return `color-mix(in oklch, var(--accent) ${Math.max(8, Math.round(v * 100))}%, transparent)`;
}

/** 客户端 CSV 下载 */
function downloadCsv(filename: string, rows: (string | number)[][]) {
  const csv = rows
    .map((r) => r.map((c) => {
      const s = String(c ?? '');
      return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    }).join(','))
    .join('\n');
  const blob = new Blob(['﻿' + csv], { type: 'text/csv;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/* ── 页面 ───────────────────────────────────────────────────────────── */

export default function AnalyticsPage() {
  const [searchParams, setSearchParams] = useSearchParams();

  const days = createMemo<number>(() => {
    const raw = parseInt(searchParams.days as string, 10);
    return DAYS_ALLOWED.includes(raw as typeof DAYS_ALLOWED[number]) ? raw : 7;
  });
  const winParams = createMemo(() => ({ days: days() }));
  const setDays = (v: string) => setSearchParams({ days: v });

  const [wordSort, setWordSort] = createSignal<WordSort>('count');

  // 各聚合端点独立兜底：单接口 5xx/超时不外抛，下游各自兜空态，避免整页白屏。
  const [kpi] = createResource<AnalyticsKpiSummary | null, { days: number }>(
    winParams, (p) => adminApi.analyticsKpiSummary(p).catch(() => null),
  );
  const [funnel] = createResource<AnalyticsFunnel | null, { days: number }>(
    winParams, (p) => adminApi.analyticsFunnel(p).catch(() => null),
  );
  const [retention] = createResource<AnalyticsRetentionMatrix | null>(
    () => adminApi.analyticsRetentionMatrix(7).catch(() => null),
  );
  const [qdist] = createResource<AnalyticsQuestionDistribution | null, { days: number }>(
    winParams, (p) => adminApi.analyticsQuestionDistribution(p).catch(() => null),
  );
  // 热图固定近 7 天（对齐 "7d × 24h" 标题）
  const [hourly] = createResource(
    () => adminApi.analyticsHourly(7).catch(() => null),
  );
  const [wfreq] = createResource<AnalyticsWordFrequency | null, { days: number; sort: WordSort }>(
    () => ({ days: days(), sort: wordSort() }),
    (p) => adminApi.analyticsWordFrequency({ days: p.days, sort: p.sort, limit: 12 }).catch(() => null),
  );
  const [insights] = createResource<AnalyticsInsights | null, { days: number }>(
    winParams, (p) => adminApi.analyticsInsights(p).catch(() => null),
  );
  const [wbRank] = createResource<WordbookRankRow[] | null, { days: number }>(
    winParams, (p) => adminApi.analyticsWordbookRank({ days: p.days }).then((r) => r.rows).catch(() => null),
  );

  // hourly：算出峰值用于相对配色 + 观察结论
  const heat = createMemo(() => {
    const h = hourly();
    if (!h) return null;
    let peak = 0;
    let peakDow = 0;
    let peakHour = 0;
    h.matrix.forEach((row, dow) => row.forEach((v, hr) => {
      if (v > peak) { peak = v; peakDow = dow; peakHour = hr; }
    }));
    // values 按展示顺序（周一→周日）归一化到 0-1
    const values: number[][] = DOW_ORDER.map((dow) =>
      (h.matrix[dow] ?? Array(24).fill(0)).map((v) => (peak > 0 ? v / peak : 0)),
    );
    const raw: number[][] = DOW_ORDER.map((dow) => h.matrix[dow] ?? Array(24).fill(0));
    return { values, raw, peak, peakDow, peakHour, total: h.total };
  });

  // ── 导出全部 CSV：汇总已加载端点 ──────────────────────────────────
  function exportAll() {
    const rows: (string | number)[][] = [['section', 'metric', 'value']];
    const k = kpi();
    if (k) {
      rows.push(['kpi', '新注册', k.newRegistrations.value]);
      rows.push(['kpi', '日活均值', k.dauAverage.value]);
      rows.push(['kpi', 'D7留存', (k.d7Retention.value * 100).toFixed(1) + '%']);
      rows.push(['kpi', '学习时长(秒)', k.studyDurationSecs.value]);
    }
    const f = funnel();
    if (f) f.steps.forEach((s) => rows.push(['funnel', s.label, `${s.count}|${(s.pct * 100).toFixed(1)}%`]));
    const r = retention();
    if (r) r.cohorts.forEach((c) => rows.push([
      'cohort', c.cohortStart,
      `${c.size}|${c.cells.map((x) => (x == null ? '-' : (x * 100).toFixed(0))).join('/')}`,
    ]));
    const q = qdist();
    if (q) {
      q.questionTypes.forEach((t) => rows.push(['question_type', t.label, `${t.count}|${(t.pct * 100).toFixed(1)}%`]));
      q.difficultyBins.forEach((b) => rows.push(['difficulty_bin', b.label, `${b.count}|${(b.pct * 100).toFixed(1)}%`]));
    }
    const w = wfreq();
    if (w) w.rows.forEach((x) => rows.push([
      'word_top', x.spelling,
      `count=${x.recordCount}|acc=${x.accuracy == null ? '-' : (x.accuracy * 100).toFixed(0) + '%'}|elo=${x.elo ?? '-'}|mastery=${x.mastery == null ? '-' : x.mastery.toFixed(2)}`,
    ]));
    const wbr = wbRank();
    if (wbr) wbr.forEach((x) => rows.push([
      'wordbook_rank', x.name,
      `learners=${x.learnerCount}|records=${x.recordCount}|correct=${x.correctCount}|acc=${x.accuracy == null ? '-' : (x.accuracy * 100).toFixed(0) + '%'}`,
    ]));
    if (rows.length === 1) { toast.warning('暂无可导出数据', '请等待图表加载完成'); return; }
    downloadCsv(`wordforge-analytics-${new Date().toISOString().slice(0, 10)}.csv`, rows);
    toast.success(`已导出分析报表 CSV`, `${rows.length - 1} 行`);
  }

  return (
    <div>
      <PageHead
        title="数据分析"
        desc="增长漏斗、留存矩阵、题型分布与高频词的多维分析，含自动洞察。"
        right={
          <>
            <Seg options={DAY_OPTIONS} value={String(days())} onChange={setDays} />
            <Btn variant="secondary" icon="download" onClick={exportAll}>导出</Btn>
          </>
        }
      />

      {/* ── KPI 概览 ─────────────────────────────────────────────── */}
      <div
        class="fade-up"
        style={sx({ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(210px,1fr))', gap: 16, marginBottom: 16 })}
      >
        <Show when={kpi()} fallback={<For each={[0, 1, 2, 3]}>{() => <Skel h={116} />}</For>}>
          {(k) => (
            <>
              <StatCard
                tone="accent" label="新注册" icon="users"
                value={fmtNum(k().newRegistrations.value)}
                delta={pctDelta(k().newRegistrations)} deltaLabel="较上周期"
              />
              <StatCard
                tone="success" label="日活均值" icon="zap"
                value={fmtNum(k().dauAverage.value)}
                delta={pctDelta(k().dauAverage)} deltaLabel="较上周期"
              />
              <StatCard
                tone="info" label="D7 留存" icon="refresh"
                value={(k().d7Retention.value * 100).toFixed(1)} unit="%"
                delta={ptDelta(k().d7Retention)} deltaLabel="百分点变化"
              />
              <StatCard
                tone="warning" label="学习时长" icon="clock"
                value={fmtDur(k().studyDurationSecs.value)}
                delta={pctDelta(k().studyDurationSecs)} deltaLabel="较上周期"
              />
            </>
          )}
        </Show>
      </div>

      {/* ── 增长漏斗 + 本期洞察 ──────────────────────────────────── */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.3fr) minmax(0,1fr)', gap: 16, marginBottom: 16 })}
      >
        <Panel
          title="增长漏斗"
          sub={funnel() ? `最大流失 ${funnel()!.biggestDropFrom} → ${funnel()!.biggestDropTo} (-${(funnel()!.biggestDropPt * 100).toFixed(1)}pt)` : ''}
        >
          <Show when={funnel()} fallback={<Loading />}>
            {(f) => (
              <Show
                when={f().steps.length > 0}
                fallback={<div class="muted" style={sx({ padding: '24px 0', textAlign: 'center', fontSize: 13 })}>本窗口暂无注册队列</div>}
              >
                <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12 })}>
                  <For each={f().steps}>
                    {(s) => {
                      const col = funnelColor(s.tone);
                      return (
                        <div>
                          <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline', marginBottom: 4 })}>
                            <span style={sx({ fontSize: 13, fontWeight: 600 })}>
                              {s.label}{' '}
                              <span class="muted-3" style={sx({ fontWeight: 400, fontSize: 11 })}>{s.sublabel}</span>
                            </span>
                            <span class="mono" style={sx({ fontSize: 12.5 })}>
                              {fmtNum(s.count)} · {(s.pct * 100).toFixed(0)}%
                              {/* delta 固定宽度列：空值也占位，保证 count·pct 在各行对齐 */}
                              <span style={sx({ display: 'inline-block', minWidth: 68, textAlign: 'left', marginLeft: 6, color: s.deltaPt == null ? 'transparent' : s.deltaPt >= 0 ? 'var(--success)' : 'var(--error)' })}>
                                <Show when={s.deltaPt != null}>
                                  {s.deltaPt! >= 0 ? '▲' : '▼'}{Math.abs(s.deltaPt! * 100).toFixed(1)}pt
                                </Show>
                              </span>
                            </span>
                          </div>
                          <div style={sx({ height: 26, borderRadius: 8, background: 'var(--surface-sunken)', overflow: 'hidden' })}>
                            <div style={sx({ width: (s.pct * 100) + '%', height: '100%', background: `color-mix(in oklch, ${col} 75%, transparent)`, transition: 'width .6s var(--ease)' })} />
                          </div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </Show>
            )}
          </Show>
        </Panel>

        <Panel title="本期洞察" sub="启发式自动生成">
          <Show when={insights()} fallback={<Loading />}>
            {(ins) => (
              <Show
                when={ins().items.length > 0}
                fallback={<div class="muted" style={sx({ padding: '24px 0', textAlign: 'center', fontSize: 13 })}>暂无洞察</div>}
              >
                <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
                  <For each={ins().items}>
                    {(it) => {
                      const col = INSIGHT_COLOR[it.tone];
                      return (
                        <div style={sx({ padding: 12, borderRadius: 11, background: 'var(--surface-sunken)', borderLeft: `3px solid ${col}` })}>
                          <div style={sx({ fontWeight: 700, fontSize: 12.5, color: col })}>{it.title}</div>
                          <div class="muted" style={sx({ fontSize: 12, marginTop: 4, lineHeight: 1.55 })}>{it.body}</div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </Show>
            )}
          </Show>
        </Panel>
      </div>

      {/* ── 周 cohort 留存矩阵 ───────────────────────────────────── */}
      <Panel title="周 cohort 留存矩阵" sub="按注册周分组" style={{ 'margin-bottom': '16px' }}>
        <Show when={retention()} fallback={<Loading />}>
          {(r) => (
            <Show
              when={r().cohorts.length > 0}
              fallback={<div class="muted" style={sx({ padding: '24px 0', textAlign: 'center', fontSize: 13 })}>暂无 cohort 样本</div>}
            >
              <div style={sx({ overflowX: 'auto' })}>
                <table class="tbl" style={sx({ minWidth: 560 })}>
                  <thead>
                    <tr>
                      <th>注册周</th>
                      <th>规模</th>
                      <For each={Array.from({ length: r().weeks }, (_, i) => i)}>{(i) => <th>W{i}</th>}</For>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={r().cohorts}>
                      {(c) => (
                        <tr>
                          <td class="mono" style={sx({ fontSize: 11.5 })}>{c.cohortStart.slice(5)}</td>
                          <td class="mono">{fmtNum(c.size)}</td>
                          <For each={c.cells}>
                            {(v) => (
                              <td style={sx({ padding: 4 })}>
                                <Show when={v != null} fallback={<span class="muted-3">—</span>}>
                                  <span style={sx({ display: 'block', textAlign: 'center', padding: '4px 0', borderRadius: 5, fontSize: 11, fontWeight: 600, background: cohortBg(v as number), color: (v as number) > 0.5 ? '#fff' : 'var(--text)' })}>
                                    {((v as number) * 100).toFixed(0)}
                                  </span>
                                </Show>
                              </td>
                            )}
                          </For>
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

      {/* ── 题型分布 + 高频词 ────────────────────────────────────── */}
      <div
        class="grid-collapse"
        style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16, marginBottom: 16 })}
      >
        <Panel title="题型分布" sub={qdist() ? `${fmtNum(qdist()!.totalRecords)} 条记录` : ''}>
          <Show when={qdist()} fallback={<Loading />}>
            {(q) => {
              const donutData = (): DonutDatum[] =>
                q().questionTypes.map((t, i) => ({ label: t.label, value: t.count, color: QTYPE_COLORS[i % QTYPE_COLORS.length] }));
              return (
                <>
                  <Show
                    when={q().questionTypes.length > 0}
                    fallback={<div class="muted" style={sx({ padding: '8px 0', fontSize: 12.5 })}>题型未标注，暂无可分布数据</div>}
                  >
                    <Donut size={150} thickness={22} centerValue={q().questionTypes.length} centerLabel="题型" data={donutData()} />
                  </Show>
                  <div style={sx({ marginTop: 14 })}>
                    <div class="eyebrow" style={sx({ marginBottom: 8 })}>难度分箱（ELO）</div>
                    <BarChart horizontal data={q().difficultyBins.map((b) => ({ label: b.label, value: b.count }))} fmtV={(v) => fmtNum(v)} />
                  </div>
                </>
              );
            }}
          </Show>
        </Panel>

        <Panel
          title="高频词"
          sub="窗口内答题次数 Top"
          right={
            <select
              class="select"
              style={sx({ width: 'auto', padding: '5px 9px', fontSize: 12 })}
              value={wordSort()}
              onChange={(e) => setWordSort(e.currentTarget.value as WordSort)}
            >
              <option value="count">按次数</option>
              <option value="accuracy">按正确率</option>
              <option value="elo">按 ELO</option>
              <option value="mastery">按掌握度</option>
            </select>
          }
        >
          <Show when={wfreq()} fallback={<Loading />}>
            {(w) => (
              <Show
                when={w().rows.length > 0}
                fallback={<div class="muted" style={sx({ padding: '24px 0', textAlign: 'center', fontSize: 13 })}>本窗口无答题记录</div>}
              >
                <div style={sx({ overflowX: 'auto' })}>
                  <table class="tbl">
                    <thead>
                      <tr>
                        <th>#</th><th>单词</th><th>次数</th><th>正确率</th><th>ELO</th><th>掌握</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={w().rows}>
                        {(row: AnalyticsWordFrequencyRow) => (
                          <tr>
                            <td class="mono muted-3">{row.rank}</td>
                            <td>
                              <b>{row.spelling}</b>{' '}
                              <Show when={row.pos}><span class="muted-3" style={sx({ fontSize: 11 })}>{row.pos}</span></Show>
                            </td>
                            <td class="mono">{fmtNum(row.recordCount)}</td>
                            <td class="mono">{row.accuracy == null ? '—' : `${(row.accuracy * 100).toFixed(0)}%`}</td>
                            <td class="mono">{row.elo ?? '—'}</td>
                            <td class="mono">{row.mastery == null ? '—' : `${(row.mastery * 100).toFixed(0)}%`}</td>
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
      </div>

      {/* ── 词库排行 ──────────────────────────────────────────────── */}
      <Panel title="词库排行" sub="按答题量排序" style={{ 'margin-bottom': '16px' }}>
        <Show when={wbRank()} fallback={<Loading />}>
          {(rows) => (
            <Show
              when={rows().length > 0}
              fallback={<div class="muted" style={sx({ padding: '24px 0', textAlign: 'center', fontSize: 13 })}>本窗口暂无词库答题样本</div>}
            >
              <div style={sx({ overflowX: 'auto' })}>
                <table class="tbl">
                  <thead>
                    <tr>
                      <th>词库</th><th>学习者</th><th>答题数</th><th>正确数</th><th>正确率</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={rows()}>
                      {(r: WordbookRankRow) => (
                        <tr>
                          <td style={sx({ fontWeight: 600 })}>{r.name}</td>
                          <td class="mono">{fmtNum(r.learnerCount)}</td>
                          <td class="mono">{fmtNum(r.recordCount)}</td>
                          <td class="mono">{fmtNum(r.correctCount)}</td>
                          <td>
                            <Show when={r.accuracy != null} fallback={<span class="muted-3">—</span>}>
                              <div style={sx({ display: 'flex', alignItems: 'center', gap: 8 })}>
                                <div class="bar" style={sx({ height: 6, width: 70 })}>
                                  <i style={sx({ width: (r.accuracy! * 100) + '%', background: 'var(--success)' }) as JSX.CSSProperties} />
                                </div>
                                <span class="mono">{(r.accuracy! * 100).toFixed(0)}%</span>
                              </div>
                            </Show>
                          </td>
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

      {/* ── 学习时段热图 7d × 24h ────────────────────────────────── */}
      <Panel
        title="学习时段热图 · 7d × 24h"
        sub="单元值 = 当小时活跃次数（相对峰值着色）"
        right={
          <Show when={heat() && heat()!.peak > 0}>
            <span class="muted-3 mono" style={sx({ fontSize: 11.5 })}>
              峰值 周{WEEKDAY_CN[heat()!.peakDow]} {String(heat()!.peakHour).padStart(2, '0')}:00 · {fmtNum(heat()!.peak)}
            </span>
          </Show>
        }
      >
        <Show when={heat()} fallback={<Loading />}>
          {(h) => (
            <Show
              when={h().peak > 0}
              fallback={<div class="muted" style={sx({ padding: '24px 0', textAlign: 'center', fontSize: 13 })}>本窗口无答题样本</div>}
            >
              <Heatmap
                rows={7}
                cols={24}
                values={h().values}
                rowLabels={DOW_ORDER.map((d) => `周${WEEKDAY_CN[d]}`)}
                colLabels={HOUR_LABELS}
                colorFor={heatColor}
                fmtCell={(r, c) => `周${WEEKDAY_CN[DOW_ORDER[r]]} ${String(c).padStart(2, '0')}:00 · ${fmtNum(h().raw[r]?.[c] ?? 0)} 次`}
              />
              <div class="muted" style={sx({ marginTop: 12, fontSize: 12, lineHeight: 1.55 })}>
                观察：最活跃时段在周{WEEKDAY_CN[h().peakDow]} {String(h().peakHour).padStart(2, '0')}:00（{fmtNum(h().peak)} 次活跃）。
                据此安排推送 / 维护窗口可避开学习高峰。
              </div>
            </Show>
          )}
        </Show>
      </Panel>

    </div>
  );
}
