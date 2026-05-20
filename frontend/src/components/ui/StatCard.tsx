import { Show, createMemo } from 'solid-js';
import { Card } from './Card';
import { useCountUp } from '@/lib/motion';

interface StatCardProps {
  title: string;
  value: string | number;
  icon: string;
  color: 'accent' | 'success' | 'warning' | 'error' | 'info';
  trend?: { value: number; label: string };
  /** value 是 number 时启用 count-up 滚动动画，默认 true */
  animate?: boolean;
  /** 数字格式化（仅 value 是 number 时生效） */
  format?: (v: number) => string;
  /** 紧凑尺寸：多卡 grid（≥6 列）场景用 'sm'，默认 'md' */
  size?: 'sm' | 'md';
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

  // 数字 count-up：仅当 value 是 number 且 animate !== false 时启用
  const numericTarget = createMemo(() => (typeof props.value === 'number' ? props.value : null));
  const countUpEnabled = () => numericTarget() !== null && props.animate !== false;
  const animatedDisplay = useCountUp({
    to: () => numericTarget() ?? 0,
    format: props.format,
  });
  const displayValue = () => (countUpEnabled() ? animatedDisplay() : String(props.value));

  const isSmall = () => props.size === 'sm';
  const styles = () =>
    isSmall()
      ? {
          padding: 'md' as const,
          gap: 'gap-3',
          iconBox: 'w-8 h-8',
          iconSvg: 'w-5 h-5',
          value: 'text-xl',
          title: 'text-xs',
        }
      : {
          padding: 'lg' as const,
          gap: 'gap-4',
          iconBox: 'w-10 h-10',
          iconSvg: 'w-6 h-6',
          value: 'text-3xl',
          title: 'text-sm',
        };

  return (
    <Card variant="interactive" padding={styles().padding} class="h-full">
      <div class={`flex items-start ${styles().gap}`}>
        {/* icon 缺失时降级为色块 dot，避免空 svg 让卡片看起来"图标丢失" */}
        <div class={`${styles().iconBox} shrink-0 rounded-lg flex items-center justify-center shadow-elevation-1 ${colors().bg}`}>
          <Show
            when={props.icon}
            fallback={<span aria-hidden="true" class={`w-2 h-2 rounded-full ${colors().text.replace('text-', 'bg-')}`} />}
          >
            <svg class={`${styles().iconSvg} ${colors().text}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round" d={props.icon} />
            </svg>
          </Show>
        </div>
        <div class="flex-1 min-w-0">
          <p class={`${styles().value} font-bold tabular-nums truncate ${colors().text}`}>{displayValue()}</p>
          <p class={`${styles().title} text-content-secondary truncate`}>{props.title}</p>
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
