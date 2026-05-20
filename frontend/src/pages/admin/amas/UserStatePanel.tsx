import { createResource, createSignal, createMemo, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { EChart } from '@/components/ui/EChart';
import { adminApi, type AmasUserStateHistogram } from '@/api/admin';
import { chartColor } from '@/lib/chartTheme';

const FIELD_TONE: Record<string, Parameters<typeof chartColor>[0]> = {
  attention: 'accent',
  fatigue: 'error',
  motivation: 'success',
  confidence: 'warning',
};

const FIELD_LABEL: Record<string, string> = {
  attention: '注意力',
  fatigue: '疲劳',
  motivation: '动机',
  confidence: '信心',
};

/** C4: User state 实时分布直方图 — 4 个并列 histogram */
export function UserStatePanel() {
  const [days, setDays] = createSignal<1 | 7 | 30>(1);
  const [bins, setBins] = createSignal(20);
  const [dist] = createResource(
    () => ({ d: days(), b: bins() }),
    async ({ d, b }) => adminApi.amasUserStateDistribution(d, b),
  );

  const histOption = (h: AmasUserStateHistogram): import('echarts').EChartsOption => {
    const span = h.max - h.min;
    const labels = h.bins.map((_, i) => {
      const lo = h.min + (span * i) / h.bins.length;
      return lo.toFixed(2);
    });
    return {
      grid: { left: 40, right: 16, top: 16, bottom: 24 },
      tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
      xAxis: { type: 'category', data: labels, axisLabel: { fontSize: 11, interval: Math.floor(h.bins.length / 6), rotate: 30 } },
      yAxis: { type: 'value', minInterval: 1 },
      series: [
        {
          type: 'bar',
          data: h.bins,
          itemStyle: { color: chartColor(FIELD_TONE[h.field] ?? 'muted') },
          barCategoryGap: '8%',
        },
      ],
    };
  };

  return (
    <Card variant="elevated">
      <div class="flex items-baseline justify-between mb-3 flex-wrap gap-2">
        <h2 class="text-lg font-semibold text-content">用户状态分布</h2>
        <div class="flex items-center gap-3 flex-wrap">
          <div class="flex items-center gap-1 text-xs" role="group" aria-label="统计窗口">
            <span class="text-content-tertiary">窗口</span>
            <For each={[1, 7, 30] as const}>
              {(n) => (
                <button
                  type="button"
                  onClick={() => setDays(n)}
                  aria-pressed={days() === n}
                  aria-label={`使用 ${n} 天窗口`}
                  class={`focus-ring-soft px-2 py-1 rounded-md transition-colors ${
                    days() === n ? 'bg-accent text-accent-content' : 'bg-surface-secondary text-content-secondary hover:text-content'
                  }`}
                >
                  {n}天
                </button>
              )}
            </For>
          </div>
          <div class="flex items-center gap-1 text-xs" role="group" aria-label="直方图桶数">
            <span class="text-content-tertiary">桶数</span>
            <For each={[10, 20, 40] as const}>
              {(n) => (
                <button
                  type="button"
                  onClick={() => setBins(n)}
                  aria-pressed={bins() === n}
                  aria-label={`使用 ${n} 个桶`}
                  class={`focus-ring-soft px-2 py-1 rounded-md transition-colors ${
                    bins() === n ? 'bg-accent text-accent-content' : 'bg-surface-secondary text-content-secondary hover:text-content'
                  }`}
                >
                  {n}
                </button>
              )}
            </For>
          </div>
        </div>
      </div>

      <Show
        when={!dist.loading && dist()}
        keyed
        fallback={<div class="min-h-[440px] flex items-center justify-center"><Spinner /></div>}
      >
        {(d) => {
          const fields: AmasUserStateHistogram[] = [d.attention, d.fatigue, d.motivation, d.confidence];
          return (
            <Show when={fields[0].sampleSize > 0} fallback={<Empty title="窗口内无采样" description="monitoring.sampleRate=0.05 默认 5% 采样；尝试拉长窗口" />}>
              <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
                <For each={fields}>
                  {(h) => {
                    const option = createMemo(() => histOption(h));
                    return (
                      <Card variant="outlined" padding="sm">
                        <div class="flex items-baseline justify-between mb-2 gap-2 flex-wrap">
                          <span class="text-sm font-medium text-content">{FIELD_LABEL[h.field]}</span>
                          <span class="text-[10px] text-content-tertiary tabular-nums">
                            n={h.sampleSize} · 均值 {h.mean.toFixed(3)} · 中位 {h.median.toFixed(3)}
                          </span>
                        </div>
                        <EChart option={option} height="180px" />
                      </Card>
                    );
                  }}
                </For>
              </div>

              <p class="text-xs text-content-tertiary mt-3 tabular-nums">
                冷启动样本：explore <code class="font-mono">{d.coldStartExplore}</code>{' / '}
                exploit <code class="font-mono">{d.coldStartExploit}</code>
              </p>
            </Show>
          );
        }}
      </Show>
    </Card>
  );
}
