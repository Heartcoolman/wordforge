import { createMemo, Show } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import type { PatchCanary, PatchCanaryStatus } from '@/api/admin';

const STATUS_LABEL: Record<PatchCanaryStatus, string> = {
  active: '灰度中', effective: '已生效', rolled_back: '已回滚',
};
const STATUS_VARIANT: Record<PatchCanaryStatus, 'warning' | 'success' | 'error'> = {
  active: 'warning', effective: 'success', rolled_back: 'error',
};

function fmtDelta(v: number): string {
  return `${v >= 0 ? '+' : ''}${v.toFixed(2)}`;
}

export function PatchCanaryCard(props: {
  c: PatchCanary;
  steps: [number, number, number];
  busy: boolean;
  onScale: (percent: number) => void;
  onRollback: () => void;
  onPromote: () => void;
}) {
  // 下一档：steps 里第一个严格大于当前 percent 的档位
  const nextStep = createMemo<number | null>(() => {
    const s = props.steps.find((p) => p > props.c.percent);
    return s ?? null;
  });
  const rewardDelta = createMemo(() => props.c.liveReward - props.c.baselineReward);

  return (
    <Card variant="elevated">
      <div class="flex items-baseline justify-between gap-3 mb-2 flex-wrap">
        <div class="flex items-center gap-2">
          <span class="font-mono text-sm text-content">{props.c.versionHash.slice(0, 10)}</span>
          <Badge variant={STATUS_VARIANT[props.c.status]} size="sm" dot>{STATUS_LABEL[props.c.status]}</Badge>
          <span class="text-xs text-content-tertiary">建议 #{props.c.suggestionId}</span>
        </div>
        <span class="text-xs text-content-tertiary tabular-nums">
          cohort [{props.c.cohortLo}, {props.c.cohortHi})
        </span>
      </div>

      {/* 百分比条 */}
      <div class="mb-3">
        <div class="flex items-baseline justify-between mb-1">
          <span class="text-xs text-content-secondary">灰度覆盖</span>
          <span class="text-sm font-medium text-content tabular-nums">{props.c.percent}%</span>
        </div>
        <div class="h-2 rounded-full bg-surface-tertiary overflow-hidden">
          <div
            class="h-full rounded-full bg-gradient-accent-strong transition-[width] duration-base"
            style={{ width: `${props.c.percent}%` }}
          />
        </div>
      </div>

      {/* live stat-pill 对比 baseline */}
      <div class="grid grid-cols-2 gap-2 mb-3">
        <div class="rounded-lg bg-surface-secondary px-3 py-2">
          <p class="text-[11px] text-content-tertiary">实测 reward</p>
          <p class="text-sm font-medium tabular-nums text-content">
            {props.c.liveReward.toFixed(3)}
            <span class={`ml-1 text-xs ${rewardDelta() >= 0 ? 'text-success' : 'text-error'}`}>
              {fmtDelta(rewardDelta())}
            </span>
          </p>
        </div>
        <div class="rounded-lg bg-surface-secondary px-3 py-2">
          <p class="text-[11px] text-content-tertiary">实测异常率</p>
          <p class={`text-sm font-medium tabular-nums ${props.c.liveAnomalyRate > 0.05 ? 'text-error' : 'text-content'}`}>
            {(props.c.liveAnomalyRate * 100).toFixed(2)}%
          </p>
        </div>
      </div>

      <Show when={props.c.status === 'active'}>
        <div class="flex gap-2 justify-end">
          <Button size="sm" variant="outline" loading={props.busy} onClick={props.onRollback}>回滚</Button>
          <Show when={nextStep() != null && props.c.percent < 100}>
            <Button size="sm" variant="secondary" loading={props.busy} onClick={() => props.onScale(nextStep()!)}>
              扩量到 {nextStep()}%
            </Button>
          </Show>
          <Show when={props.c.percent >= 100}>
            <Button size="sm" loading={props.busy} onClick={props.onPromote}>提升为 stable</Button>
          </Show>
        </div>
      </Show>
    </Card>
  );
}
