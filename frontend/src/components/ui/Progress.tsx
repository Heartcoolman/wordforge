import { Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface ProgressBarProps {
  value: number;
  max?: number;
  size?: 'sm' | 'md' | 'lg';
  color?: 'accent' | 'success' | 'warning' | 'error' | 'info';
  showLabel?: boolean;
  /** 启用 striped 动画（适合"进行中"长任务） */
  striped?: boolean;
  class?: string;
}

const heightMap = { sm: 'h-1.5', md: 'h-2.5', lg: 'h-4' };
// 渐变填充：顶层标准色 → 底层 hover 色，layered 感更精致
const fillMap = {
  accent: 'bg-gradient-to-b from-accent to-accent-hover',
  success: 'bg-gradient-to-b from-success to-success/80',
  warning: 'bg-gradient-to-b from-warning to-warning/80',
  error: 'bg-gradient-to-b from-error to-error/80',
  info: 'bg-gradient-to-b from-info to-info/80',
};

export function ProgressBar(props: ProgressBarProps) {
  const percent = () => Math.min(100, Math.max(0, (props.value / (props.max ?? 100)) * 100));

  return (
    <div class={cn('w-full', props.class)}>
      <div class={cn('w-full bg-surface-tertiary rounded-full overflow-hidden', heightMap[props.size ?? 'md'])}
        role="progressbar"
        aria-valuenow={props.value}
        aria-valuemin={0}
        aria-valuemax={props.max ?? 100}
      >
        <div
          class={cn(
            'h-full rounded-full transition-[width] duration-slow ease-out-expo relative overflow-hidden',
            fillMap[props.color ?? 'accent'],
          )}
          style={{ width: `${percent()}%` }}
        >
          <Show when={props.striped}>
            {/* striped overlay — 45° 斜条纹滑动，提示活动中 */}
            <span aria-hidden="true" class="absolute inset-0 animate-progress-stripe" />
          </Show>
        </div>
      </div>
      <Show when={props.showLabel}>
        <div class="flex justify-between mt-1">
          <span class="text-xs text-content-secondary tabular-nums">{props.value}/{props.max ?? 100}</span>
          <span class="text-xs text-content-secondary tabular-nums">{percent().toFixed(0)}%</span>
        </div>
      </Show>
    </div>
  );
}

interface CircularProgressProps {
  value: number;
  max?: number;
  size?: number;
  strokeWidth?: number;
  color?: string;
  class?: string;
}

export function CircularProgress(props: CircularProgressProps) {
  const size = () => props.size ?? 48;
  const stroke = () => props.strokeWidth ?? 4;
  const radius = () => (size() - stroke()) / 2;
  const circumference = () => 2 * Math.PI * radius();
  const percent = () => Math.min(100, Math.max(0, (props.value / (props.max ?? 100)) * 100));
  const offset = () => circumference() - (percent() / 100) * circumference();

  return (
    <div class={cn('relative inline-flex items-center justify-center', props.class)}
      role="progressbar"
      aria-valuenow={props.value}
      aria-valuemin={0}
      aria-valuemax={props.max ?? 100}
    >
      <svg width={size()} height={size()} class="-rotate-90">
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          stroke="var(--surface-tertiary)"
          stroke-width={stroke()}
          fill="none"
        />
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          stroke={props.color ?? 'var(--accent)'}
          stroke-width={stroke()}
          fill="none"
          stroke-linecap="round"
          stroke-dasharray={String(circumference())}
          stroke-dashoffset={offset()}
          class="transition-[stroke-dashoffset] duration-slow ease-out-expo"
        />
      </svg>
      <span class="absolute text-xs font-medium text-content tabular-nums">
        {percent().toFixed(0)}%
      </span>
    </div>
  );
}
