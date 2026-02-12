import { Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface ProgressBarProps {
  value: number;
  max?: number;
  size?: 'sm' | 'md' | 'lg';
  color?: 'accent' | 'success' | 'warning' | 'error' | 'info';
  showLabel?: boolean;
  class?: string;
}

const heightMap = { sm: 'h-1.5', md: 'h-2.5', lg: 'h-4' };
const colorMap = {
  accent: 'bg-accent',
  success: 'bg-success',
  warning: 'bg-warning',
  error: 'bg-error',
  info: 'bg-info',
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
            'h-full rounded-full transition-all duration-500 ease-out',
            colorMap[props.color ?? 'accent'],
          )}
          style={{ width: `${percent()}%` }}
        />
      </div>
      <Show when={props.showLabel}>
        <div class="flex justify-between mt-1">
          <span class="text-xs text-content-secondary">{props.value}/{props.max ?? 100}</span>
          <span class="text-xs text-content-secondary">{percent().toFixed(0)}%</span>
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
          class="transition-all duration-500 ease-out"
        />
      </svg>
      <span class="absolute text-xs font-medium text-content">
        {percent().toFixed(0)}%
      </span>
    </div>
  );
}
