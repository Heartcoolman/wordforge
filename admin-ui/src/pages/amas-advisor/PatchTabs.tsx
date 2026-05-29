import { For, createSignal, onCleanup, onMount } from 'solid-js';

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
  /** 测试可注入；默认走每秒 tick 的实时时钟 */
  nowMs?: number;
}) {
  // 每秒 tick 让倒计时真实走动；测试注入 nowMs 时不依赖时钟，确定可测。
  const [tick, setTick] = createSignal(0);
  onMount(() => {
    const t = setInterval(() => setTick((n) => n + 1), 1000);
    onCleanup(() => clearInterval(t));
  });
  const now = () => (props.nowMs ?? (tick(), Date.now()));
  return (
    <div class="flex items-center justify-between border-b border-border-hairline">
      <div role="tablist" class="tabs">
        <For each={TABS}>
          {(t) => (
            <button
              type="button"
              role="tab"
              aria-selected={props.active === t.id}
              onClick={() => props.onChange(t.id)}
              class={`tab${props.active === t.id ? ' is-active' : ''}`}
            >
              <span>{t.label}</span>
              <span class="count">{props.counts[t.id]}</span>
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
