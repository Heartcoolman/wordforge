import { Show, createMemo } from 'solid-js';
import { Card } from './Card';
import { Sparkline } from './Sparkline';
import { useCountUp } from '@/lib/motion';

interface StatCardProps {
  title: string;
  value: string | number;
  icon: string;
  color: 'accent' | 'success' | 'warning' | 'error' | 'info';
  trend?: { value: number; label: string; showZero?: boolean };
  /** value 是 number 时启用 count-up 滚动动画，默认 true */
  animate?: boolean;
  /** 数字格式化（仅 value 是 number 时生效） */
  format?: (v: number) => string;
  /** 紧凑尺寸：多卡 grid（≥6 列）场景用 'sm'，默认 'md' */
  size?: 'sm' | 'md';
  /** 是否可交互（启用 Card hover 抬升）；StatCard 默认非交互卡 */
  interactive?: boolean;
  /** 可选趋势 sparkline 数据，显示在卡底部右侧；颜色随 color 主题 */
  spark?: number[];
}

const colorMap = {
  accent: { bg: 'bg-accent-light', text: 'text-accent', dot: 'bg-accent', sparkVar: '--accent' },
  success: { bg: 'bg-success-light', text: 'text-success', dot: 'bg-success', sparkVar: '--success' },
  warning: { bg: 'bg-warning-light', text: 'text-warning', dot: 'bg-warning', sparkVar: '--warning' },
  error: { bg: 'bg-error-light', text: 'text-error', dot: 'bg-error', sparkVar: '--error' },
  info: { bg: 'bg-info-light', text: 'text-info', dot: 'bg-info', sparkVar: '--info' },
};

export function StatCard(props: StatCardProps) {
  const colors = () => colorMap[props.color];

  const trendDisplay = () => {
    if (!props.trend) return null;
    const v = props.trend.value;
    // 0 默认不展示（除非业务方显式 showZero）
    if (v === 0 && !props.trend.showZero) return null;
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
          // 紧凑场景适度放大，但避免长数字被裁切
          value: 'text-xl md:text-2xl',
          title: 'text-xs',
        }
      : {
          padding: 'lg' as const,
          gap: 'gap-4',
          iconBox: 'w-10 h-10',
          iconSvg: 'w-6 h-6',
          value: 'text-2xl md:text-3xl',
          title: 'text-sm',
        };

  return (
    <Card variant={props.interactive ? 'interactive' : 'elevated'} padding={styles().padding} class="h-full">
      {/* icon + 文本 baseline 用 items-center 避免长 title 多行时 icon 偏移 */}
      <div class={`flex items-center ${styles().gap}`}>
        {/* icon 缺失时降级为色块 dot，避免空 svg 让卡片看起来"图标丢失" */}
        <div class={`${styles().iconBox} shrink-0 rounded-lg flex items-center justify-center shadow-elevation-1 ${colors().bg}`}>
          <Show
            when={props.icon}
            fallback={<span aria-hidden="true" class={`w-2 h-2 rounded-full ${colors().dot}`} />}
          >
            <svg class={`${styles().iconSvg} ${colors().text}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round" d={props.icon} />
            </svg>
          </Show>
        </div>
        <div class="flex-1 min-w-0">
          {/* 去掉 truncate 让长数字 wrap，避免截尾 */}
          <p class={`${styles().value} font-bold tabular-nums ${colors().text}`}>{displayValue()}</p>
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
      <Show when={props.spark && props.spark.length >= 2}>
        <div class="mt-3 flex justify-end opacity-90">
          <Sparkline
            data={props.spark!}
            w={84}
            h={24}
            stroke={`var(${colors().sparkVar})`}
          />
        </div>
      </Show>
    </Card>
  );
}
