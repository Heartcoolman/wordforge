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
  /** SR 文本，默认"加载中" */
  label?: string;
}

// 半径与对应周长精确计算 — 270° 弧 = circumference * 0.75
const TRACK_R = 10;
const SPIN_R = 9;
const SPIN_C = 2 * Math.PI * SPIN_R;
const SPIN_DASH = `${SPIN_C * 0.75} ${SPIN_C}`;

/**
 * 双环 Spinner — 外环为 270° 弧顺时针旋转，内圈是更细的 track（视觉锚点）
 * spin ring 半径 r=9，track r=10，避免两层 stroke 同半径互相覆盖出现毛刺。
 */
export function Spinner(props: SpinnerProps) {
  return (
    <span
      class={cn('relative inline-flex items-center justify-center', sizeMap[props.size ?? 'md'], props.class)}
      role="status"
      aria-live="polite"
      aria-label={props.label ?? '加载中'}
    >
      <span class="sr-only">{props.label ?? '加载中'}</span>
      {/* Track ring — 静态 */}
      <svg class="absolute inset-0 w-full h-full text-content-tertiary/30" fill="none" viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r={TRACK_R} stroke="currentColor" stroke-width="2.5" />
      </svg>
      {/* Spin ring — 旋转的 270° 弧（cap=round 更柔和），半径比 track 小 1 避免毛刺 */}
      <svg class="absolute inset-0 w-full h-full text-accent animate-spin" fill="none" viewBox="0 0 24 24" aria-hidden="true">
        <circle
          cx="12"
          cy="12"
          r={SPIN_R}
          stroke="currentColor"
          stroke-width="2.5"
          stroke-linecap="round"
          stroke-dasharray={SPIN_DASH}
        />
      </svg>
    </span>
  );
}
