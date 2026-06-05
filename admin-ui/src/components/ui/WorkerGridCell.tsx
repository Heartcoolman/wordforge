import { Show, createMemo } from 'solid-js';
import { cn } from '@/utils/cn';

export type WorkerOutcome = 'ok' | 'error' | 'idle' | 'unknown';

interface WorkerGridCellProps {
  /** Worker 名称，如 "amas_decision_loop" */
  name: string;
  /** 最近一次执行结果。'error' 触发红色 ring-pulse */
  outcome?: WorkerOutcome;
  /** 最近一次执行的 ISO timestamp 或 epoch ms */
  lastRunAt?: string | number | null;
  /** 最近一次耗时（ms） */
  lastDurationMs?: number | null;
  /** 错误概述，error outcome 时展示 */
  lastError?: string | null;
  /** 自定义点击行为（如打开 detail drawer） */
  onClick?: () => void;
  class?: string;
}

const dotColor: Record<WorkerOutcome, string> = {
  ok: 'bg-success',
  error: 'bg-error',
  idle: 'bg-content-tertiary',
  unknown: 'bg-content-quaternary',
};

const ringPulseColor: Record<WorkerOutcome, string> = {
  ok: '',
  error: 'shadow-[0_0_0_0_var(--error)] animate-ring-pulse text-error',
  idle: '',
  unknown: '',
};

/**
 * 监控页面"21 worker 健康网格"的单个 cell。
 *
 * 视觉：dot（颜色按 outcome）+ name + lastRun 相对时间 + lastDuration mono 数值。
 * error 时 dot 走 ring-pulse 红光警示。点击可触发 detail。
 *
 * 网格在父侧用 `grid-cols-2/3/5` 自适应。
 */
export function WorkerGridCell(props: WorkerGridCellProps) {
  const outcome = () => props.outcome ?? 'unknown';
  const isInteractive = () => typeof props.onClick === 'function';

  const relTime = createMemo(() => {
    const t = props.lastRunAt;
    if (t == null) return '—';
    const ts = typeof t === 'string' ? Date.parse(t) : t;
    if (Number.isNaN(ts)) return '—';
    const diff = Date.now() - ts;
    if (diff < 0) return '即将';
    const s = Math.floor(diff / 1000);
    if (s < 60) return `${s}s 前`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m 前`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h 前`;
    const d = Math.floor(h / 24);
    return `${d}d 前`;
  });

  const baseClass = cn(
    'rounded-lg bg-surface-elevated shadow-elevation-1 px-3 py-2.5 text-left',
    'transition-[transform,box-shadow] duration-fast ease-out-expo',
    isInteractive() && 'hover:-translate-y-0.5 hover:shadow-elevation-2 cursor-pointer focus-ring-soft',
    outcome() === 'error' && 'ring-1 ring-error/30',
    props.class,
  );

  const innerContent = () => (
    <>
      <div class="flex items-center gap-2">
        <span
          aria-hidden="true"
          class={cn('inline-block size-2 rounded-full shrink-0', dotColor[outcome()], ringPulseColor[outcome()])}
        />
        <span class="truncate text-[12.5px] font-medium text-content">{props.name}</span>
      </div>
      <div class="mt-1.5 flex items-center justify-between gap-2 text-[11px] text-content-tertiary">
        <span>{relTime()}</span>
        <Show when={typeof props.lastDurationMs === 'number'}>
          <span class="font-mono tabular-nums">{props.lastDurationMs}ms</span>
        </Show>
      </div>
      <Show when={outcome() === 'error' && props.lastError}>
        <div class="mt-1 truncate text-[11px] text-error-strong" title={props.lastError!}>
          {props.lastError}
        </div>
      </Show>
    </>
  );

  return (
    <Show
      when={isInteractive()}
      fallback={
        <div class={baseClass} title={props.lastError || undefined}>
          {innerContent()}
        </div>
      }
    >
      <button type="button" onClick={props.onClick} class={baseClass} title={props.lastError || undefined}>
        {innerContent()}
      </button>
    </Show>
  );
}
