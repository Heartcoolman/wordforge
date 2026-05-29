import { createMemo, Show } from 'solid-js';
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
  const nextStep = createMemo<number | null>(() => props.steps.find((p) => p > props.c.percent) ?? null);
  const rewardDelta = createMemo(() => props.c.liveReward - props.c.baselineReward);

  return (
    <div class="patch-card">
      <div class="patch-head">
        <div class="avatar">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3" /><path d="M12 1v6M12 17v6M4.2 4.2l4.3 4.3M15.5 15.5l4.3 4.3M1 12h6M17 12h6" /></svg>
        </div>
        <div class="info">
          <div class="title">建议 #{props.c.suggestionId} · {props.c.versionHash.slice(0, 10)}</div>
          <div class="meta">
            <span class="font-mono tabular-nums">cohort [{props.c.cohortLo}, {props.c.cohortHi})</span>
            <span class="dot" />
            <span>24h 监测窗口 · reward/异常率 阈值自动回滚</span>
          </div>
        </div>
        <Badge variant={STATUS_VARIANT[props.c.status]} size="sm" dot>{STATUS_LABEL[props.c.status]}</Badge>
      </div>

      {/* canary 色带：百分比 + track + 实测 */}
      <div class={`canary-bar${props.c.percent >= 100 ? ' is-100' : ''}`}>
        <span class="pct">灰度 {props.c.percent}%</span>
        <div class="track"><span style={{ width: `${Math.min(props.c.percent, 100)}%` }} /></div>
        <span class="tabular-nums" style={{ 'font-size': '11.5px' }}>
          reward {props.c.liveReward.toFixed(3)}
          <span class={rewardDelta() >= 0 ? 'text-success' : 'text-error'}> {fmtDelta(rewardDelta())}</span>
          {' · '}异常 {(props.c.liveAnomalyRate * 100).toFixed(1)}%
        </span>
      </div>

      <Show when={props.c.status === 'active'}>
        <div class="patch-foot">
          <Button size="sm" variant="outline" loading={props.busy} onClick={props.onRollback}>回滚</Button>
          <Show when={nextStep() != null && props.c.percent < 100}>
            <Button size="sm" variant="secondary" loading={props.busy} onClick={() => props.onScale(nextStep()!)}>扩量到 {nextStep()}%</Button>
          </Show>
          <Show when={props.c.percent < 100}>
            <Button size="sm" variant="secondary" loading={props.busy} onClick={() => props.onScale(100)}>直接 100% 生效</Button>
          </Show>
          <Show when={props.c.percent >= 100}>
            <Button size="sm" loading={props.busy} onClick={props.onPromote}>提升为 stable</Button>
          </Show>
        </div>
      </Show>
    </div>
  );
}
