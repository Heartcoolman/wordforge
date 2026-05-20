import { Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface MiniStatProps {
  label: string;
  value: string | number;
  trend?: { value: number; label: string };
  tone?: 'accent' | 'success' | 'warning' | 'info' | 'error';
  /** 是否启用 hover 抬升（仅可点击场景）。默认 false */
  interactive?: boolean;
}

const toneMap: Record<NonNullable<MiniStatProps['tone']>, string> = {
  accent: 'text-accent',
  success: 'text-success',
  warning: 'text-warning',
  info: 'text-info',
  error: 'text-error',
};

export function MiniStat(props: MiniStatProps) {
  const tone = () => toneMap[props.tone ?? 'accent'];
  const trendClass = (v: number) =>
    v > 0 ? 'text-success' : v < 0 ? 'text-error' : 'text-content-tertiary';
  const trendArrow = (v: number) => (v > 0 ? '↑' : v < 0 ? '↓' : '→');

  return (
    <div
      class={cn(
        'rounded-lg bg-surface-secondary border border-border-hairline p-3 min-w-0',
        // 仅在 interactive=true 时启用 hover 抬升，避免静态卡误导用户可点击
        props.interactive
          ? 'transition-[transform,box-shadow] duration-base ease-out-expo hover:-translate-y-0.5 hover:shadow-elevation-1 cursor-pointer'
          : '',
      )}
    >
      <p class="text-caption font-medium text-content-secondary mb-1 truncate" title={props.label}>{props.label}</p>
      {/* 去 truncate；tabular-nums 保证数字对齐 */}
      <p class={cn('text-2xl font-bold tracking-tight tabular-nums', tone())} title={String(props.value)}>{props.value}</p>
      <Show when={props.trend}>
        {(t) => (
          <p class={cn('text-xs mt-1.5 flex items-center gap-1 tabular-nums', trendClass(t().value))}>
            <span>{trendArrow(t().value)}</span>
            <span>{Math.abs(t().value)}%</span>
            <span class="text-content-tertiary ml-1">{t().label}</span>
          </p>
        )}
      </Show>
    </div>
  );
}
