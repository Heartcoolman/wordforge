import { For } from 'solid-js';

export type PatchTabId = 'pending' | 'canary' | 'effective' | 'rejected';

export interface PatchCounts {
  pending: number;
  canary: number;
  effective: number;
  rejected: number;
}

const POLL_PERIOD_MS = 20 * 60 * 1000;

const TABS: Array<{ id: PatchTabId; label: string }> = [
  { id: 'pending', label: '待审' },
  { id: 'canary', label: '灰度中' },
  { id: 'effective', label: '已生效' },
  { id: 'rejected', label: '已拒绝' },
];

function countdownText(nowMs: number): string {
  const remain = POLL_PERIOD_MS - (nowMs % POLL_PERIOD_MS);
  const totalSec = Math.ceil(remain / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m} 分 ${s} 秒`;
}

export function PatchTabs(props: {
  active: PatchTabId;
  counts: PatchCounts;
  onChange: (id: PatchTabId) => void;
  /** 测试可注入；默认 Date.now() */
  nowMs?: number;
}) {
  const now = () => props.nowMs ?? Date.now();
  return (
    <div class="flex items-center justify-between border-b border-border-hairline">
      <div role="tablist" class="flex gap-1">
        <For each={TABS}>
          {(t) => (
            <button
              type="button"
              role="tab"
              aria-selected={props.active === t.id}
              onClick={() => props.onChange(t.id)}
              class={`px-3 py-2 text-sm flex items-center gap-1.5 border-b-2 -mb-px transition-colors ${
                props.active === t.id
                  ? 'border-accent text-accent'
                  : 'border-transparent text-content-secondary hover:text-content'
              }`}
            >
              <span>{t.label}</span>
              <span class="text-[11px] tabular-nums px-1.5 rounded-full bg-surface-secondary text-content-tertiary">
                {props.counts[t.id]}
              </span>
            </button>
          )}
        </For>
      </div>
      <span class="text-[11.5px] text-content-tertiary tabular-nums pr-1">
        下次巡查 · {countdownText(now())}
      </span>
    </div>
  );
}
