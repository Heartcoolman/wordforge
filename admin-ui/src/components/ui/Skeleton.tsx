import { cn } from '@/utils/cn';

interface SkeletonProps {
  class?: string;
  width?: string;
  height?: string;
  rounded?: boolean;
  /** sr-only 文本，默认"加载中" */
  label?: string;
}

export function Skeleton(props: SkeletonProps) {
  return (
    <div
      role="status"
      aria-live="polite"
      aria-busy="true"
      class={cn(
        // shimmer 自带 gradient 背景 + 横向位移动画，替代 animate-pulse 的简单透明度脉冲
        'animate-shimmer',
        props.rounded ? 'rounded-full' : 'rounded-lg',
        props.class,
      )}
      style={{
        width: props.width ?? '100%',
        height: props.height ?? '1rem',
      }}
    >
      <span class="sr-only">{props.label ?? '加载中'}</span>
    </div>
  );
}

export function CardSkeleton() {
  return (
    <div role="status" aria-live="polite" aria-busy="true" class="p-5 rounded-xl bg-surface-elevated shadow-elevation-1 space-y-3">
      <span class="sr-only">加载中</span>
      {/* 行高与真实卡片层次对齐：title 1.5rem，body 1rem，footer 1rem */}
      <Skeleton width="40%" height="1.5rem" />
      <Skeleton width="70%" height="1rem" />
      <Skeleton width="55%" height="1rem" />
      <Skeleton width="30%" height="1rem" />
    </div>
  );
}
