import { Show } from 'solid-js';
import { Card } from './Card';

interface StatCardProps {
  title: string;
  value: string | number;
  icon: string;
  color: 'accent' | 'success' | 'warning' | 'error' | 'info';
  trend?: { value: number; label: string };
}

const colorMap = {
  accent: { bg: 'bg-accent-light', text: 'text-accent' },
  success: { bg: 'bg-success-light', text: 'text-success' },
  warning: { bg: 'bg-warning-light', text: 'text-warning' },
  error: { bg: 'bg-error-light', text: 'text-error' },
  info: { bg: 'bg-info-light', text: 'text-info' },
};

export function StatCard(props: StatCardProps) {
  const colors = () => colorMap[props.color];

  const trendDisplay = () => {
    if (!props.trend) return null;
    const v = props.trend.value;
    if (v > 0) return { arrow: '↑', class: 'text-success', text: `${v}%` };
    if (v < 0) return { arrow: '↓', class: 'text-error', text: `${Math.abs(v)}%` };
    return { arrow: '→', class: 'text-content-tertiary', text: '0%' };
  };

  return (
    <Card variant="interactive" padding="lg">
      <div class="flex items-start gap-4">
        <div class={`w-10 h-10 rounded-lg flex items-center justify-center shadow-elevation-1 ${colors().bg}`}>
          <svg class={`w-6 h-6 ${colors().text}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
            <path stroke-linecap="round" stroke-linejoin="round" d={props.icon} />
          </svg>
        </div>
        <div class="flex-1 min-w-0">
          <p class={`text-3xl font-bold tabular-nums ${colors().text}`}>{props.value}</p>
          <p class="text-sm text-content-secondary">{props.title}</p>
          <Show when={trendDisplay()}>
            {(t) => (
              <p class={`text-xs mt-1 tabular-nums ${t().class}`}>
                {t().arrow} {t().text} {props.trend!.label}
              </p>
            )}
          </Show>
        </div>
      </div>
    </Card>
  );
}
