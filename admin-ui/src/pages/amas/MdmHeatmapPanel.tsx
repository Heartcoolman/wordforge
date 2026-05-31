import { createResource, For, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi } from '@/api/admin';

/** 遗忘概率 0..1 → 6 档热度（-1 空白）。 */
function level(v: number): number {
  if (v < 0) return 0;
  return Math.min(6, Math.max(0, Math.ceil(v * 6)));
}

/** MDM 遗忘概率热图：7d × 14 难度段（无词频 rank，用 words.difficulty 段真实近似）。
 * forget = 1 - mastery_level，数据来自 /api/admin/amas/metrics/mdm-heatmap。 */
export function MdmHeatmapPanel(props: { days: () => number }) {
  const [d] = createResource(props.days, (days) => adminApi.amasMdmHeatmap(days));

  return (
    <Card variant="elevated" class="amas-panel">
      <div class="amas-panel-head">
        <h3>MDM 遗忘概率热图</h3>
        <span class="amas-sub">{props.days()}d × 14 难度段</span>
      </div>
      <Show when={!d.loading} fallback={<div class="min-h-[200px] flex items-center justify-center"><Spinner /></div>}>
        <Show when={(d()?.days?.length ?? 0) > 0} fallback={<Empty title="暂无 MDM 状态" description="word_learning_states 当前为空" />}>
          <div class="heat-grid">
            <For each={d()?.days ?? []}>
              {(day, di) => (
                <>
                  <div class="row-l">{day.slice(5)}</div>
                  <For each={d()?.cells[di()] ?? []}>
                    {(cell) => <div class="h" data-v={cell >= 0 ? level(cell) : undefined} title={cell >= 0 ? cell.toFixed(2) : '无样本'} />}
                  </For>
                </>
              )}
            </For>
          </div>
          <div class="flex justify-between text-content-tertiary text-xs mt-3">
            <span>低难度词</span><span>高难度词</span>
          </div>
          <div class="amas-divider" />
          <div class="flex justify-between text-sm">
            <span class="text-content-secondary">峰值遗忘概率</span>
            <strong class="font-mono tabular-nums">{(d()?.peak ?? 0).toFixed(2)}</strong>
          </div>
        </Show>
      </Show>
    </Card>
  );
}
