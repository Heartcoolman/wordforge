import { Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface MiniStatProps {
  label: string;
  value: string | number;
  trend?: { value: number; label: string };
  tone?: 'accent' | 'success' | 'warning' | 'info' | 'error';
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
    <div class="rounded-lg bg-surface-secondary border border-border p-3">
      <p class="text-xs font-medium text-content-secondary mb-1">{props.label}</p>
      <p class={cn('text-2xl font-bold tracking-tight', tone())}>{props.value}</p>
      <Show when={props.trend}>
        {(t) => (
          <p class={cn('text-xs mt-1.5 flex items-center gap-1', trendClass(t().value))}>
            <span>{trendArrow(t().value)}</span>
            <span>{Math.abs(t().value)}%</span>
            <span class="text-content-tertiary ml-1">{t().label}</span>
          </p>
        )}
      </Show>
    </div>
  );
}
