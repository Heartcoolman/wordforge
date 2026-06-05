import { createMemo, createResource, createSignal, Show, For } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { adminApi } from '@/api/admin';
import type {
  AnalyticsWordFrequencyRow, AnalyticsInsightItem,
} from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { formatNumber, formatDuration } from '@/utils/formatters';
import './analytics.css';

const DAYS_ALLOWED = [7, 14, 30, 90] as const;
// 后端 hourly matrix 行序按 strftime('%w'):0=周日。WEEKDAY_CN 按后端行索引取中文;
// DOW_ORDER 控制展示顺序为 周一→周日(对齐设计图视觉)。
const WEEKDAY_CN = ['日', '一', '二', '三', '四', '五', '六'];
const DOW_ORDER = [1, 2, 3, 4, 5, 6, 0];

type WordSort = 'count' | 'accuracy' | 'elo' | 'mastery';
const WORD_SORT_TABS: Array<{ key: WordSort; label: string }> = [
  { key: 'count', label: '答题数' },
  { key: 'accuracy', label: '错误率' },
  { key: 'elo', label: '难度' },
  { key: 'mastery', label: '掌握度' },
];

/** 漏斗原始 SQL（运维参考文档，非伪造数据；与后端 /analytics/funnel 同口径） */
const FUNNEL_SQL = `-- 注册到长期留存漏斗（窗口内注册队列）
WITH cohort AS (
  SELECT id AS user_id
  FROM users
  WHERE created_at >= :range_start AND created_at < :range_end
)
SELECT 'register' AS stage, COUNT(*) AS users FROM cohort
UNION ALL SELECT 'pick_wordbook', COUNT(DISTINCT c.user_id)
  FROM cohort c JOIN user_wordbooks uw ON uw.user_id = c.user_id
UNION ALL SELECT 'first_answer', COUNT(DISTINCT c.user_id)
  FROM cohort c JOIN learning_records r ON r.user_id = c.user_id
UNION ALL SELECT 'd1_return', COUNT(DISTINCT c.user_id)
  FROM cohort c JOIN learning_records r ON r.user_id = c.user_id
  AND r.created_at >= datetime(:reg_at, '+1 day')
ORDER BY stage;`;

/** dist-row fill 配色（按组 / 序号轮换，贴设计稿） */
const QTYPE_TONE = ['', 'is-success', 'is-info', 'is-warning', 'is-violet'];

/** 把 0-1 小数 → 整数百分比映射到 cohort r-* 档 */
function cohortTier(v: number | null): string {
  if (v == null) return 'r-na';
  const pct = v * 100;
  if (pct >= 90) return 'r-100';
  if (pct >= 60) return 'r-70';
  if (pct >= 40) return 'r-50';
  if (pct >= 25) return 'r-30';
  if (pct >= 10) return 'r-15';
  return 'r-5';
}

/** 词频正确率 bar 配色：< 45% error，< 60% warn，否则 success */
function accTone(acc: number | null): string {
  if (acc == null) return '';
  if (acc < 0.45) return 'is-error';
  if (acc < 0.6) return 'is-warn';
  return '';
}

function pctDeltaText(deltaPct: number | null): { text: string; cls: string } {
  if (deltaPct == null) return { text: '—', cls: '' };
  const up = deltaPct >= 0;
  return { text: `${up ? '▲' : '▼'} ${Math.abs(deltaPct * 100).toFixed(1)}%`, cls: up ? 'up' : 'down' };
}
function ptDeltaText(deltaPt: number | null): { text: string; cls: string } {
  if (deltaPt == null) return { text: '—', cls: '' };
  const up = deltaPt >= 0;
  return { text: `${up ? '▲' : '▼'} ${Math.abs(deltaPt * 100).toFixed(1)} pt`, cls: up ? 'up' : 'down' };
}
/** 漏斗每步 delta（百分点）展示 */
function funnelDelta(deltaPt: number | null): { text: string; cls: string } {
  if (deltaPt == null) return { text: '基准', cls: '' };
  const up = deltaPt >= 0;
  return { text: `${up ? '▲' : '▼'} ${Math.abs(deltaPt * 100).toFixed(1)} pt`, cls: up ? 'up' : 'down' };
}

const INSIGHT_ICON: Record<AnalyticsInsightItem['tone'], string> = {
  success: 'M20 6 9 17l-5-5',
  warning: 'M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4M12 17h.01',
  info: 'M12 16v-4M12 8h.01',
  accent: 'M22 12h-4l-3 9L9 3l-3 9H2',
};

/** 客户端 CSV 下载（单卡用） */
function downloadCsv(filename: string, rows: (string | number)[][]) {
  const csv = rows
    .map((r) =>
      r
        .map((c) => {
          const s = String(c ?? '');
          return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
        })
        .join(','),
    )
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

export default function AnalyticsPage() {
  const [searchParams, setSearchParams] = useSearchParams();

  const days = createMemo(() => {
    const raw = parseInt(searchParams.days as string, 10);
    return DAYS_ALLOWED.includes(raw as typeof DAYS_ALLOWED[number]) ? raw : 7;
  });
  const from = createMemo(() => (searchParams.from as string) || undefined);
  const to = createMemo(() => (searchParams.to as string) || undefined);
  /** 用 from/to 还是 days：from+to 都给时优先窗口区间 */
  const usingRange = createMemo(() => Boolean(from() && to()));
  /** 窗口参数对象 —— 任一 createResource 的 source；变更即触发重取 */
  const winParams = createMemo(() =>
    usingRange() ? { from: from(), to: to() } : { days: days() },
  );

  const setDays = (d: number) => setSearchParams({ days: d.toString(), from: undefined, to: undefined });
  const applyRange = (f: string, t: string) => setSearchParams({ from: f, to: t });

  // —— 词频排序（本地 signal，独立驱动 word-frequency 重取）——
  const [wordSort, setWordSort] = createSignal<WordSort>('count');

  // —— 区间浮层 ——
  const [rangeOpen, setRangeOpen] = createSignal(false);
  const [draftFrom, setDraftFrom] = createSignal(from() ?? '');
  const [draftTo, setDraftTo] = createSignal(to() ?? '');

  // —— SQL / 离开用户决策序列 浮层 ——
  const [sqlOpen, setSqlOpen] = createSignal(false);
  const [leaversOpen, setLeaversOpen] = createSignal(false);

  // —— resources（fetcher 内 .catch(() => null) 兜底：任一聚合端点 5xx/超时不外抛，
  // 下游 Show when={…} 各自兜空态，避免单接口抖动经全局 ErrorBoundary 放大为整站白屏）——
  const [kpi] = createResource(winParams, (p) => adminApi.analyticsKpiSummary(p).catch(() => null));
  const [funnel] = createResource(winParams, (p) => adminApi.analyticsFunnel(p).catch(() => null));
  const [matrix] = createResource(() => adminApi.analyticsRetentionMatrix(7).catch(() => null));
  const [dist] = createResource(winParams, (p) => adminApi.analyticsQuestionDistribution(p).catch(() => null));
  // 热图固定近 7 天(对齐设计图"7d × 24h"标题),不随窗口/区间变化
  const [hourly] = createResource(() => adminApi.analyticsHourly(7).catch(() => null));
  const [words] = createResource(
    () => ({ ...winParams(), sort: wordSort() }),
    (p) => adminApi.analyticsWordFrequency({ ...p, limit: 10 }).catch(() => null),
  );
  const [insights] = createResource(winParams, (p) => adminApi.analyticsInsights(p).catch(() => null));

  const windowLabel = createMemo(() =>
    usingRange() ? `${from()} ~ ${to()}` : `${days()} 天`,
  );

  // —— 导出全部 CSV：汇总已加载的各端点 ——
  function exportCsv() {
    const rows: string[][] = [['section', 'metric', 'value']];
    const k = kpi();
    if (k) {
      rows.push(['kpi', 'newRegistrations', String(k.newRegistrations.value)]);
      rows.push(['kpi', 'dauAverage', String(k.dauAverage.value)]);
      rows.push(['kpi', 'd7Retention', (k.d7Retention.value * 100).toFixed(1) + '%']);
      rows.push(['kpi', 'studyDurationSecs', String(k.studyDurationSecs.value)]);
    }
    const f = funnel();
    if (f) {
      f.steps.forEach((s) => {
        rows.push(['funnel', s.label, `${s.count}|${(s.pct * 100).toFixed(1)}%`]);
      });
    }
    const m = matrix();
    if (m) {
      m.cohorts.forEach((c) => {
        rows.push(['cohort', c.cohortStart, `${c.size}|${c.cells.map((x) => (x == null ? '-' : (x * 100).toFixed(0))).join('/')}`]);
      });
    }
    const d = dist();
    if (d) {
      d.questionTypes.forEach((q) => rows.push(['question_type', q.label, `${q.count}|${(q.pct * 100).toFixed(1)}%`]));
      d.difficultyBins.forEach((b) => rows.push(['difficulty_bin', b.label, `${b.count}|${(b.pct * 100).toFixed(1)}%`]));
    }
    const w = words();
    if (w) {
      w.rows.forEach((r) => {
        rows.push(['word_top', r.spelling, `count=${r.recordCount}|acc=${r.accuracy == null ? '-' : (r.accuracy * 100).toFixed(0) + '%'}|elo=${r.elo ?? '-'}|mastery=${r.mastery == null ? '-' : r.mastery.toFixed(2)}`]);
      });
    }
    if (rows.length === 1) {
      uiStore.toast.warning('没有可导出的数据，请等待图表全部加载');
      return;
    }
    downloadCsv(`wordforge-analytics-${new Date().toISOString().slice(0, 10)}.csv`, rows);
    uiStore.toast.success(`已导出 ${rows.length - 1} 行 CSV`);
  }

  // —— 单卡 CSV ——
  function exportFunnelCsv() {
    const f = funnel();
    if (!f || f.steps.length === 0) { uiStore.toast.warning('漏斗暂无数据'); return; }
    downloadCsv('analytics-funnel.csv', [
      ['stage', 'label', 'sublabel', 'count', 'pct', 'deltaPt'],
      ...f.steps.map((s) => [s.key, s.label, s.sublabel, s.count, (s.pct * 100).toFixed(1) + '%', s.deltaPt == null ? '' : (s.deltaPt * 100).toFixed(1) + 'pt']),
    ]);
  }
  function exportCohortCsv() {
    const m = matrix();
    if (!m || m.cohorts.length === 0) { uiStore.toast.warning('cohort 暂无数据'); return; }
    downloadCsv('analytics-cohort.csv', [
      ['cohort', 'size', ...Array.from({ length: m.weeks }, (_, i) => `w${i}`)],
      ...m.cohorts.map((c) => [c.cohortStart, c.size, ...c.cells.map((x) => (x == null ? '' : (x * 100).toFixed(0)))]),
    ]);
  }
  function exportWordsCsv() {
    const w = words();
    if (!w || w.rows.length === 0) { uiStore.toast.warning('词频暂无数据'); return; }
    downloadCsv('analytics-word-frequency.csv', [
      ['rank', 'word', 'pos', 'recordCount', 'accuracy', 'elo', 'mastery'],
      ...w.rows.map((r) => [
        r.rank, r.spelling, r.pos ?? '', r.recordCount,
        r.accuracy == null ? '' : (r.accuracy * 100).toFixed(0) + '%',
        r.elo ?? '',
        r.mastery == null ? '' : r.mastery.toFixed(2),
      ]),
    ]);
  }

  // —— hourly 热图分档：相对峰值映射 lvl-0..5 ——
  const heatCells = createMemo(() => {
    const h = hourly();
    if (!h) return null;
    let peak = 0;
    let peakDow = 0;
    let peakHour = 0;
    h.matrix.forEach((row, dow) =>
      row.forEach((v, hr) => {
        if (v > peak) { peak = v; peakDow = dow; peakHour = hr; }
      }),
    );
    return { matrix: h.matrix, peak, peakDow, peakHour, total: h.total };
  });

  return (
    <div class="analytics">
      <div class="stack">
        {/* —— 页头 —— */}
        <div class="page-header">
          <div class="row spread wrap">
            <div>
              <h1 class="page-title">数据分析</h1>
              <p class="page-desc">
                从注册到长期留存的全链路漏斗 · 7 周 cohort 留存矩阵 · 学习时段热图 · 词频 top。
                每张卡片可单独导出 CSV，漏斗可查看原始 SQL。
              </p>
            </div>
            <div class="row gap-2 wrap">
              <div class="segmented" role="group" aria-label="时间窗口">
                <For each={DAYS_ALLOWED}>
                  {(d) => (
                    <button
                      class={!usingRange() && days() === d ? 'is-active' : ''}
                      onClick={() => setDays(d)}
                    >
                      {d} 天
                    </button>
                  )}
                </For>
              </div>

              <div class="range-pop-wrap">
                <button
                  class={`btn btn-secondary ${usingRange() ? 'is-on' : ''}`}
                  onClick={() => {
                    setDraftFrom(from() ?? '');
                    setDraftTo(to() ?? '');
                    setRangeOpen((v) => !v);
                  }}
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="18" rx="2" /><line x1="16" y1="2" x2="16" y2="6" /><line x1="8" y1="2" x2="8" y2="6" /><line x1="3" y1="10" x2="21" y2="10" /></svg>
                  自定义区间
                </button>
                <Show when={rangeOpen()}>
                  <div class="range-pop">
                    <div class="field">
                      <label for="rng-from">起 (from)</label>
                      <input id="rng-from" type="date" value={draftFrom()} onInput={(e) => setDraftFrom(e.currentTarget.value)} />
                    </div>
                    <div class="field">
                      <label for="rng-to">止 (to)</label>
                      <input id="rng-to" type="date" value={draftTo()} onInput={(e) => setDraftTo(e.currentTarget.value)} />
                    </div>
                    <div class="actions">
                      <button class="btn btn-ghost btn-sm" onClick={() => setRangeOpen(false)}>取消</button>
                      <button
                        class="btn btn-primary btn-sm"
                        disabled={!draftFrom() || !draftTo()}
                        onClick={() => {
                          if (!draftFrom() || !draftTo()) return;
                          applyRange(draftFrom(), draftTo());
                          setRangeOpen(false);
                        }}
                      >
                        应用
                      </button>
                    </div>
                  </div>
                </Show>
              </div>

              <button class="btn btn-primary" onClick={exportCsv}>
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></svg>
                导出全部 CSV
              </button>
            </div>
          </div>
        </div>

        {/* —— Mini KPI grid —— */}
        <div class="ministat-grid animate-fade-in-up">
          <Show when={kpi()} fallback={<KpiPlaceholder />}>
            {(k) => {
              const reg = pctDeltaText(k().newRegistrations.deltaPct);
              const dau = pctDeltaText(k().dauAverage.deltaPct);
              const ret = ptDeltaText(k().d7Retention.deltaPt);
              const dur = pctDeltaText(k().studyDurationSecs.deltaPct);
              return (
                <>
                  <div>
                    <div class="l">{days()} 天新注册</div>
                    <div class="v">{formatNumber(k().newRegistrations.value)}</div>
                    <div class={`delta ${reg.cls}`}>{reg.text} 比上窗口</div>
                  </div>
                  <div>
                    <div class="l">{days()} 天活跃 (DAU 平均)</div>
                    <div class="v">{formatNumber(k().dauAverage.value)}</div>
                    <div class={`delta ${dau.cls}`}>{dau.text}</div>
                  </div>
                  <div>
                    <div class="l">d7 留存率</div>
                    <div class="v">{(k().d7Retention.value * 100).toFixed(1)}<span class="unit">%</span></div>
                    <div class={`delta ${ret.cls}`}>{ret.text}</div>
                  </div>
                  <div>
                    <div class="l">{days()} 天累计学习时长</div>
                    <div class="v">{formatDuration(k().studyDurationSecs.value)}</div>
                    <div class={`delta ${dur.cls}`}>{dur.text}</div>
                  </div>
                </>
              );
            }}
          </Show>
        </div>

        {/* —— Funnel + Cohort —— */}
        <div class="grid cols-12">
          {/* 漏斗 */}
          <div class="panel animate-fade-in-up" style={{ 'grid-column': 'span 7' }}>
            <div class="panel-header">
              <h3>注册到长期留存 · 漏斗</h3>
              <span class="text-mono text-small">{windowLabel()}</span>
              <div class="aside">
                <button class="btn btn-ghost btn-sm" onClick={exportFunnelCsv}>CSV</button>
                <button class="btn btn-ghost btn-sm" onClick={() => setSqlOpen(true)}>
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" /></svg>
                  SQL
                </button>
              </div>
            </div>
            <div class="panel-body">
              <Show when={funnel()} fallback={<div class="empty-hint">加载中…</div>}>
                {(f) => (
                  <Show when={f().steps.length > 0} fallback={<div class="empty-hint">本窗口暂无注册队列</div>}>
                    <div class="funnel">
                      <For each={f().steps}>
                        {(step) => {
                          const d = funnelDelta(step.deltaPt);
                          const barTone = step.tone === 'good' ? 'is-good' : step.tone === 'warn' ? 'is-warn' : '';
                          return (
                            <div class="funnel-row">
                              <div class="name"><strong>{step.label}</strong><span>{step.sublabel}</span></div>
                              <div class="bar-wrap">
                                <div class={`bar ${barTone}`} style={{ width: `${(step.pct * 100).toFixed(1)}%` }} />
                                <span class="bar-label">{formatNumber(step.count)}</span>
                              </div>
                              <div class="conv">
                                <div class="pct">{(step.pct * 100).toFixed(1)}%</div>
                                <div class={`delta ${d.cls}`}>{d.text}</div>
                              </div>
                            </div>
                          );
                        }}
                      </For>
                    </div>
                    <div class="divider" />
                    <div class="row spread text-small">
                      <span class="text-secondary">
                        最大流失点：{f().biggestDropFrom} → {f().biggestDropTo}（流失 {(f().biggestDropPt * 100).toFixed(1)} pt）
                      </span>
                      <a class="text-accent drill-link" onClick={() => setLeaversOpen(true)}>
                        查看离开用户的 AMAS 决策序列 →
                      </a>
                    </div>
                  </Show>
                )}
              </Show>
            </div>
          </div>

          {/* cohort 留存矩阵 */}
          <div class="panel animate-fade-in-up" style={{ 'grid-column': 'span 5' }}>
            <div class="panel-header">
              <h3>留存矩阵 · 7 周 cohort</h3>
              <span class="text-mono text-small">按注册周分组</span>
              <div class="aside">
                <button class="btn btn-ghost btn-sm" onClick={exportCohortCsv}>CSV</button>
              </div>
            </div>
            <div class="panel-body" style={{ padding: '12px 8px 18px 18px' }}>
              <Show when={matrix()} fallback={<div class="empty-hint">加载中…</div>}>
                {(m) => (
                  <Show when={m().cohorts.length > 0} fallback={<div class="empty-hint">暂无 cohort 样本</div>}>
                    <table class="cohort-table">
                      <thead>
                        <tr>
                          <th style={{ 'text-align': 'left', 'padding-left': '12px' }}>Cohort</th>
                          <For each={Array.from({ length: m().weeks }, (_, i) => i)}>
                            {(i) => <th>w{i}</th>}
                          </For>
                        </tr>
                      </thead>
                      <tbody>
                        <For each={m().cohorts}>
                          {(c) => (
                            <tr>
                              <td class="cohort-name">
                                {c.cohortStart.slice(5)}
                                <div class="cohort-size">{formatNumber(c.size)} 人</div>
                              </td>
                              <For each={c.cells}>
                                {(cell) => {
                                  const tier = cohortTier(cell);
                                  return (
                                    <td>
                                      <span class={`cell-retention ${tier}`}>
                                        {cell == null ? '—' : (cell * 100).toFixed(0)}
                                      </span>
                                    </td>
                                  );
                                }}
                              </For>
                            </tr>
                          )}
                        </For>
                      </tbody>
                    </table>
                    <div class="divider" />
                    <div class="row gap-3 text-small">
                      <span class="text-tertiary">% · 注册后第 N 周</span>
                      <div class="row gap-1" style={{ 'margin-left': 'auto', 'align-items': 'center' }}>
                        <span style={{ 'font-size': '10px', color: 'var(--content-tertiary)' }}>低</span>
                        <For each={['r-5', 'r-15', 'r-30', 'r-50', 'r-70', 'r-100']}>
                          {(r) => <span class={`cell-retention ${r}`} style={{ 'min-width': '18px' }} />}
                        </For>
                        <span style={{ 'font-size': '10px', color: 'var(--content-tertiary)' }}>高</span>
                      </div>
                    </div>
                  </Show>
                )}
              </Show>
            </div>
          </div>
        </div>

        {/* —— 答题分布 + 学习时段热图 —— */}
        <div class="grid cols-12">
          {/* 答题分布 */}
          <div class="panel animate-fade-in-up" style={{ 'grid-column': 'span 5' }}>
            <div class="panel-header">
              <h3>答题分布</h3>
              <Show when={dist()}>
                <span class="text-mono text-small">{windowLabel()} · {formatNumber(dist()!.totalRecords)} 道题</span>
              </Show>
            </div>
            <div class="panel-body">
              <Show when={dist()} fallback={<div class="empty-hint">加载中…</div>}>
                {(d) => (
                  <>
                    <h4 class="dist-group-title">题型</h4>
                    <Show
                      when={d().questionTypes.length > 0}
                      fallback={<div class="empty-hint" style={{ padding: '8px' }}>题型未标注，暂无可分布数据</div>}
                    >
                      <For each={d().questionTypes}>
                        {(q, i) => (
                          <div class="dist-row">
                            <span class="lbl">{q.label}</span>
                            <div class="track"><div class={`fill ${QTYPE_TONE[i() % QTYPE_TONE.length]}`} style={{ width: `${(q.pct * 100).toFixed(1)}%` }} /></div>
                            <span class="v">{(q.pct * 100).toFixed(0)}%</span>
                          </div>
                        )}
                      </For>
                    </Show>

                    <div class="divider" />

                    <h4 class="dist-group-title">难度（ELO 分箱）</h4>
                    <For each={d().difficultyBins}>
                      {(b) => (
                        <div class="dist-row">
                          <span class="lbl">{b.label}</span>
                          <div class="track"><div class="fill" style={{ width: `${(b.pct * 100).toFixed(1)}%` }} /></div>
                          <span class="v">{(b.pct * 100).toFixed(0)}%</span>
                        </div>
                      )}
                    </For>
                  </>
                )}
              </Show>
            </div>
          </div>

          {/* 学习时段热图 */}
          <div class="panel animate-fade-in-up" style={{ 'grid-column': 'span 7' }}>
            <div class="panel-header">
              <h3>学习时段热图 · 7d × 24h</h3>
              <span class="text-mono text-small">单元值 = 当小时活跃</span>
              <Show when={heatCells() && heatCells()!.peak > 0}>
                <div class="aside">
                  <span class="text-small text-tertiary">
                    峰值 周{WEEKDAY_CN[heatCells()!.peakDow]} {String(heatCells()!.peakHour).padStart(2, '0')}:00 · {formatNumber(heatCells()!.peak)}
                  </span>
                </div>
              </Show>
            </div>
            <div class="panel-body">
              <Show when={heatCells()} fallback={<div class="empty-hint">加载中…</div>}>
                {(h) => (
                  <Show when={h().peak > 0} fallback={<div class="empty-hint">本窗口无答题样本</div>}>
                    <div class="heat-week">
                      <For each={DOW_ORDER}>
                        {(backendDow) => (
                          <>
                            <div class="heat-day-label">{WEEKDAY_CN[backendDow]}</div>
                            <For each={h().matrix[backendDow] ?? []}>
                              {(v, hr) => {
                                const lvl = h().peak > 0 ? Math.min(5, Math.ceil((v / h().peak) * 5)) : 0;
                                return (
                                  <div
                                    class={`heat-h lvl-${v === 0 ? 0 : lvl}`}
                                    title={`周${WEEKDAY_CN[backendDow]} ${String(hr()).padStart(2, '0')}:00 · ${v}`}
                                  />
                                );
                              }}
                            </For>
                          </>
                        )}
                      </For>
                    </div>
                    <div class="heat-axis">
                      <span>0</span><span>6</span><span>12</span><span>18</span><span>23</span>
                    </div>
                    <div class="divider" />
                    {/* 观察结论基于真实 peak 时段动态生成，无伪造 */}
                    <div class="row spread">
                      <span class="text-small text-tertiary">观察：</span>
                      <span class="text-small text-secondary" style={{ flex: '1', 'margin-left': '8px' }}>
                        最活跃时段在周{WEEKDAY_CN[h().peakDow]} {String(h().peakHour).padStart(2, '0')}:00（{formatNumber(h().peak)} 次活跃）。
                        据此安排推送 / 维护窗口可避开学习高峰。
                      </span>
                    </div>
                  </Show>
                )}
              </Show>
            </div>
          </div>
        </div>

        {/* —— 词频 Top + 本期洞察 —— */}
        <div class="grid cols-12">
          {/* 词频 Top */}
          <div class="panel animate-fade-in-up" style={{ 'grid-column': 'span 8' }}>
            <div class="panel-header">
              <h3>词频 Top</h3>
              <span class="text-mono text-small">{windowLabel()}</span>
              <div class="aside">
                <div class="segmented" role="group" aria-label="词频排序">
                  <For each={WORD_SORT_TABS}>
                    {(t) => (
                      <button class={wordSort() === t.key ? 'is-active' : ''} onClick={() => setWordSort(t.key)}>
                        {t.label}
                      </button>
                    )}
                  </For>
                </div>
                <button class="btn btn-ghost btn-sm" onClick={exportWordsCsv}>CSV</button>
              </div>
            </div>
            <div class="panel-body" style={{ padding: '8px 18px 18px' }}>
              <div class="word-row head">
                <span>#</span><span>词</span><span class="num">出现次数</span><span class="num">平均正确率</span><span class="num">{wordSort() === 'mastery' ? '掌握度' : 'ELO 难度'}</span>
              </div>
              <Show when={words()} fallback={<div class="empty-hint">加载中…</div>}>
                {(w) => (
                  <Show when={w().rows.length > 0} fallback={<div class="empty-hint">本窗口无答题记录</div>}>
                    <For each={w().rows}>
                      {(row: AnalyticsWordFrequencyRow) => (
                        <div class="word-row">
                          <span class="rank">{row.rank}</span>
                          <span class="word">
                            {row.spelling}
                            <Show when={row.pos}><span class="pos">{row.pos}</span></Show>
                          </span>
                          <span class="num">{formatNumber(row.recordCount)}</span>
                          <span class="num">
                            <span class="acc">
                              {row.accuracy == null ? '—' : `${(row.accuracy * 100).toFixed(0)}%`}
                              <Show when={row.accuracy != null}>
                                <span class="bar"><span class={accTone(row.accuracy)} style={{ width: `${((row.accuracy ?? 0) * 100).toFixed(0)}%` }} /></span>
                              </Show>
                            </span>
                          </span>
                          <span class="num">
                            <Show when={wordSort() === 'mastery'} fallback={<>{row.elo ?? '—'}</>}>
                              {row.mastery == null ? '—' : `${(row.mastery * 100).toFixed(0)}%`}
                            </Show>
                          </span>
                        </div>
                      )}
                    </For>
                  </Show>
                )}
              </Show>
            </div>
          </div>

          {/* 本期洞察 */}
          <div class="panel animate-fade-in-up" style={{ 'grid-column': 'span 4' }}>
            <div class="panel-header">
              <h3>本期洞察</h3>
              <span class="text-mono text-small">AMAS 自动生成</span>
            </div>
            <div class="panel-body" style={{ padding: '0' }}>
              <Show when={insights()} fallback={<div class="empty-hint" style={{ padding: '24px' }}>加载中…</div>}>
                {(ins) => (
                  <Show when={ins().items.length > 0} fallback={<div class="empty-hint" style={{ padding: '24px' }}>暂无洞察</div>}>
                    <For each={ins().items}>
                      {(item: AnalyticsInsightItem) => (
                        <div class="insight-item">
                          <div class={`ico is-${item.tone}`}>
                            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                              <path d={INSIGHT_ICON[item.tone]} />
                            </svg>
                          </div>
                          <div class="body">
                            <strong>{item.title}</strong>
                            <p>{item.body}</p>
                          </div>
                        </div>
                      )}
                    </For>
                  </Show>
                )}
              </Show>
            </div>
          </div>
        </div>
      </div>

      {/* 漏斗原始 SQL 浮层 */}
      <Show when={sqlOpen()}>
        <div class="analytics-modal-backdrop" onClick={() => setSqlOpen(false)}>
          <div class="analytics-modal" onClick={(e) => e.stopPropagation()}>
            <div class="row spread">
              <h3 class="text-headline">漏斗原始 SQL</h3>
              <button class="btn btn-ghost btn-sm" onClick={() => setSqlOpen(false)}>关闭</button>
            </div>
            <p class="text-small text-tertiary mt-2">与 <code>/api/admin/analytics/funnel</code> 同口径，参数 :range_start / :range_end 取当前窗口。</p>
            <pre class="sql-box">{FUNNEL_SQL}</pre>
          </div>
        </div>
      </Show>

      {/* 离开用户 AMAS 决策序列浮层 —— 诚实降级（无该关联端点，不伪造序列） */}
      <Show when={leaversOpen()}>
        <div class="analytics-modal-backdrop" onClick={() => setLeaversOpen(false)}>
          <div class="analytics-modal" onClick={(e) => e.stopPropagation()}>
            <div class="row spread">
              <h3 class="text-headline">离开用户的 AMAS 决策序列</h3>
              <button class="btn btn-ghost btn-sm" onClick={() => setLeaversOpen(false)}>关闭</button>
            </div>
            <div class="empty-hint" style={{ padding: '24px', 'margin-top': '12px' }}>
              该下钻依赖「漏斗流失用户 × AMAS 决策日志」关联端点，后端接入后此处展示每位离开用户的决策时间线。
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

/** KPI 占位 —— 保持 ministat-grid 4 格骨架 */
function KpiPlaceholder() {
  return (
    <For each={[0, 1, 2, 3]}>
      {() => (
        <div>
          <div class="l">加载中</div>
          <div class="v" style={{ color: 'var(--content-quaternary)' }}>—</div>
          <div class="delta">—</div>
        </div>
      )}
    </For>
  );
}
