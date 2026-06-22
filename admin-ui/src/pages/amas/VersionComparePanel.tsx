import { createResource, createSignal, createMemo, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Sparkline } from '@/components/ui/Sparkline';
import { adminApi, type AmasConfigVersionRow, type AmasVersionSliceExt } from '@/api/admin';

type RowDef = {
  key: keyof AmasVersionSliceExt;
  label: string;
  desc: string;
  fmt: (v: number | null) => string;
  delta: (d: number) => string;
  lowerBetter: boolean | null;
};
const signed = (x: number, dp: number) => `${x >= 0 ? '+' : ''}${x.toFixed(dp)}`;
const pct = (v: number | null) => `${((v ?? 0) * 100).toFixed(1)}%`;

// 对齐设计稿 amas-metrics.html 版本对比表的 7 行（口径以真实后端聚合为准）
const ROWS: RowDef[] = [
  { key: 'hitRate', label: '7d 命中率', desc: '用户首答对 / 决策数', fmt: pct, delta: (d) => `${signed(d * 100, 1)} pt`, lowerBetter: false },
  { key: 'p95LatencyMs', label: 'P95 决策耗时', desc: '单次路由 95 分位', fmt: (v) => `${(v ?? 0).toFixed(1)} ms`, delta: (d) => `${signed(d, 1)} ms`, lowerBetter: true },
  { key: 'fatigueRate', label: '疲劳触发率', desc: 'fatigue ≥ 阈值占比', fmt: pct, delta: (d) => `${signed(d * 100, 1)} pt`, lowerBetter: true },
  { key: 'retention7d', label: '7d 留存', desc: '窗口前段活跃用户末 7d 回归率', fmt: pct, delta: (d) => `${signed(d * 100, 1)} pt`, lowerBetter: false },
  { key: 'ensembleShare', label: 'ensemble 路由占比', desc: '8 算法分布主导率', fmt: pct, delta: (d) => `${signed(d * 100, 1)} pt`, lowerBetter: false },
  { key: 'configEpsilon', label: 'memory_model.half_life_base_epsilon', desc: '配置项快照 diff', fmt: (v) => (v == null ? '—' : v.toFixed(3)), delta: (d) => signed(d, 3), lowerBetter: null },
  { key: 'p95CompletionMs', label: 'P95 答题完成时间', desc: '用户视角 response_time 95 分位', fmt: (v) => `${((v ?? 0) / 1000).toFixed(2)} s`, delta: (d) => `${signed(d / 1000, 2)} s`, lowerBetter: true },
];

function formatTime(iso: string | null): string {
  if (!iso) return '无事件';
  try {
    return new Date(iso).toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

export function VersionComparePanel() {
  const [versionA, setVersionA] = createSignal('');
  const [versionB, setVersionB] = createSignal('');
  const [versions] = createResource(() => adminApi.amasListVersions(50));

  const initial = createMemo(() => {
    const list = versions() ?? [];
    return { a: list[1]?.versionHash ?? '', b: list[0]?.versionHash ?? '' };
  });
  const effA = () => versionA() || initial().a;
  const effB = () => versionB() || initial().b;

  const [compare] = createResource(
    () => [effA(), effB()] as const,
    async ([a, b]) => (a && b ? adminApi.amasCompareVersionsExt(a, b) : null),
  );

  const options = createMemo(() =>
    (versions() ?? []).map((v: AmasConfigVersionRow) => ({
      value: v.versionHash,
      label: `${v.versionHash.slice(0, 10)} · ${v.source}${v.note ? ` · ${v.note}` : ''}`,
    })),
  );

  const advice = (a: AmasVersionSliceExt, b: AmasVersionSliceExt): string => {
    const wins: string[] = [];
    if (b.hitRate > a.hitRate) wins.push('命中率');
    if (b.p95LatencyMs < a.p95LatencyMs) wins.push('P95 耗时');
    if (b.fatigueRate < a.fatigueRate) wins.push('疲劳触发率');
    if (b.anomalyRate < a.anomalyRate) wins.push('异常率');
    if (wins.length === 0) return 'B 版本相对 A 无明显改善，建议保持 A 或继续观察样本量。';
    return `B 版本在 ${wins.join(' / ')} 上改善；可考虑灰度 20% → 50% → 100% 推进（注意两版本样本量差异）。`;
  };

  return (
    <div class="space-y-3">
      <Show when={!versions.loading} fallback={<Card variant="elevated"><div class="flex justify-center py-12"><Spinner /></div></Card>}>
        <Show
          when={(versions() ?? []).length >= 2}
          fallback={<Card variant="elevated"><Empty title="至少需要两个版本" description="amas_config_versions 条目不足，先在 AMAS 配置页保存几次" /></Card>}
        >
          {/* cmp-header */}
          <Show when={compare()} keyed fallback={<div class="min-h-[120px] flex items-center justify-center"><Spinner /></div>}>
            {(c) => (
              <>
                <div class="cmp-header">
                  <div class="cmp-side">
                    <span class="l">基线版本 (A)</span>
                    <select aria-label="基线版本" value={effA()} onChange={(e) => setVersionA(e.currentTarget.value)}>
                      <For each={options()}>{(o) => <option value={o.value}>{o.label}</option>}</For>
                    </select>
                    <strong>{c.a.versionHash.slice(0, 10)}</strong>
                    <span class="meta">{c.a.eventCount.toLocaleString()} 决策样本 · {formatTime(c.a.lastEventAt)}</span>
                  </div>
                  <div class="cmp-vs">→ vs →</div>
                  <div class="cmp-side">
                    <span class="l">对比版本 (B)</span>
                    <select aria-label="对比版本" value={effB()} onChange={(e) => setVersionB(e.currentTarget.value)}>
                      <For each={options()}>{(o) => <option value={o.value}>{o.label}</option>}</For>
                    </select>
                    <strong>{c.b.versionHash.slice(0, 10)}</strong>
                    <span class="meta">{c.b.eventCount.toLocaleString()} 决策样本 · {formatTime(c.b.lastEventAt)}</span>
                  </div>
                </div>

                <Show
                  when={c.a.eventCount > 0 || c.b.eventCount > 0}
                  fallback={<Card variant="elevated"><Empty title="两个版本均无 monitoring 事件" description="可能版本尚未上线或落库延迟" /></Card>}
                >
                  <div class="cmp-row h">
                    <span>指标</span><span>A</span><span>B</span><span>Δ</span><span>B 趋势</span>
                  </div>
                  <For each={ROWS}>
                    {(row) => {
                      const av = c.a[row.key] as number | null;
                      const bv = c.b[row.key] as number | null;
                      const an = typeof av === 'number' ? av : null;
                      const bn = typeof bv === 'number' ? bv : null;
                      const hasDelta = an != null && bn != null;
                      const d = hasDelta ? bn! - an! : 0;
                      const cls =
                        row.lowerBetter == null || Math.abs(d) < 1e-9
                          ? 'is-flat'
                          : (row.lowerBetter ? d < 0 : d > 0)
                            ? 'is-up'
                            : 'is-down';
                      return (
                        <div class="cmp-row">
                          <div class="nm">{row.label}<div class="d">{row.desc}</div></div>
                          <div class="v">{row.fmt(an)}</div>
                          <div class="v">{row.fmt(bn)}</div>
                          <div class={`delta ${cls}`}>{hasDelta ? row.delta(d) : '—'}</div>
                          <div class="spark">
                            <Show when={(c.b.spark?.length ?? 0) > 1} fallback={<span />}>
                              <Sparkline
                                data={c.b.spark}
                                stroke="#2d6cdf"
                                strokeWidth={1.5}
                                ariaLabel={`${row.label} B 版本逐日趋势`}
                                class="w-full h-full"
                              />
                            </Show>
                          </div>
                        </div>
                      );
                    }}
                  </For>

                  <div class="cmp-advice">
                    <strong>建议：</strong> {advice(c.a, c.b)}
                  </div>
                </Show>
              </>
            )}
          </Show>
        </Show>
      </Show>
    </div>
  );
}
