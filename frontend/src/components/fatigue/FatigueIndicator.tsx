import { createMemo, Show } from 'solid-js';
import { cn } from '@/utils/cn';
import { fatigueStore, type FatigueLevel } from '@/stores/fatigue';

// 各等级对应的颜色
const levelColors: Record<FatigueLevel, { stroke: string; text: string; bg: string }> = {
  alert:    { stroke: 'text-success',    text: 'text-success',    bg: 'bg-success-light' },
  mild:     { stroke: 'text-warning',    text: 'text-warning',    bg: 'bg-warning-light' },
  moderate: { stroke: 'text-orange-500', text: 'text-orange-500', bg: 'bg-orange-100'    },
  severe:   { stroke: 'text-error',      text: 'text-error',      bg: 'bg-error-light'   },
};

// 各等级中文标签
const levelLabels: Record<FatigueLevel, string> = {
  alert:    '清醒',
  mild:     '轻度疲劳',
  moderate: '中度疲劳',
  severe:   '重度疲劳',
};

interface FatigueIndicatorProps {
  /** 尺寸，默认 32px */
  size?: number;
  /** 是否显示 tooltip 详情 */
  showTooltip?: boolean;
}

/**
 * 疲劳等级圆环指示器
 * 圆环进度表示疲劳分数 (0-100)，颜色随等级变化
 */
export function FatigueIndicator(props: FatigueIndicatorProps) {
  const size = () => props.size ?? 32;
  const strokeWidth = 3;
  const radius = () => (size() - strokeWidth) / 2;
  const circumference = () => 2 * Math.PI * radius();

  // 分数越高，圆环进度越多
  const offset = createMemo(() => {
    const progress = fatigueStore.fatigueScore() / 100;
    return circumference() * (1 - progress);
  });

  const colors = createMemo(() => levelColors[fatigueStore.fatigueLevel()]);

  return (
    <div
      class="relative group"
      title={`${levelLabels[fatigueStore.fatigueLevel()]} (${fatigueStore.fatigueScore()}分)`}
    >
      {/* SVG 圆环 */}
      <svg
        width={size()}
        height={size()}
        class="transform -rotate-90"
      >
        {/* 底圈 */}
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          fill="none"
          stroke="currentColor"
          stroke-width={strokeWidth}
          class="text-surface-tertiary"
        />
        {/* 进度圈 */}
        <circle
          cx={size() / 2}
          cy={size() / 2}
          r={radius()}
          fill="none"
          stroke="currentColor"
          stroke-width={strokeWidth}
          stroke-dasharray={String(circumference())}
          stroke-dashoffset={offset()}
          stroke-linecap="round"
          class={cn('transition-all duration-500', colors().stroke)}
        />
      </svg>

      {/* 中心分数 */}
      <div class="absolute inset-0 flex items-center justify-center">
        <span class={cn('text-[10px] font-bold', colors().text)}>
          {fatigueStore.fatigueScore()}
        </span>
      </div>

      {/* Tooltip 详细指标 */}
      <Show when={props.showTooltip !== false}>
        <div class={cn(
          'absolute top-full left-1/2 -translate-x-1/2 mt-2 z-50',
          'bg-surface-elevated rounded-lg shadow-lg border border-border',
          'px-3 py-2 text-xs whitespace-nowrap',
          'opacity-0 invisible group-hover:opacity-100 group-hover:visible',
          'transition-all duration-200',
        )}>
          <p class={cn('font-semibold mb-1', colors().text)}>
            {levelLabels[fatigueStore.fatigueLevel()]}
          </p>
          <div class="space-y-0.5 text-content-secondary">
            <p>疲劳分数: {fatigueStore.fatigueScore()}</p>
            <p>PERCLOS（闭眼时间占比）: {(fatigueStore.perclos() * 100).toFixed(1)}%</p>
            <p>眨眼频率: {fatigueStore.blinkRate().toFixed(1)} 次/分</p>
          </div>
          {/* 箭头 */}
          <div class="absolute -top-1 left-1/2 -translate-x-1/2 w-2 h-2 bg-surface-elevated border-l border-t border-border rotate-45" />
        </div>
      </Show>
    </div>
  );
}
