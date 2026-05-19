import { cn } from '@/utils/cn';

const sizeMap = {
  sm: 'w-4 h-4',
  md: 'w-6 h-6',
  lg: 'w-8 h-8',
  xl: 'w-12 h-12',
};

interface SpinnerProps {
  size?: keyof typeof sizeMap;
  class?: string;
}

/**
 * 双环 Spinner — 外环为半圆 stroke 顺时针旋转，内圈为更细的 track（视觉锚点）
 * 比单层 SVG path 更精致，CPU/GPU 开销持平。
 */
export function Spinner(props: SpinnerProps) {
  return (
    <span
      class={cn('relative inline-flex items-center justify-center', sizeMap[props.size ?? 'md'], props.class)}
      role="status"
      aria-label="加载中"
    >
      {/* Track ring — 静态 */}
      <svg class="absolute inset-0 w-full h-full text-content-tertiary/30" fill="none" viewBox="0 0 24 24">
        <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2.5" />
      </svg>
      {/* Spin ring — 旋转的 270° 弧（cap=round 更柔和） */}
      <svg class="absolute inset-0 w-full h-full text-accent animate-spin" fill="none" viewBox="0 0 24 24">
        <circle
          cx="12"
          cy="12"
          r="10"
          stroke="currentColor"
          stroke-width="2.5"
          stroke-linecap="round"
          stroke-dasharray="47 63"
        />
      </svg>
    </span>
  );
}
