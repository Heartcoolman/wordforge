import { cn } from '@/utils/cn';

interface SkeletonProps {
  class?: string;
  width?: string;
  height?: string;
  rounded?: boolean;
}

export function Skeleton(props: SkeletonProps) {
  return (
    <div
      aria-hidden="true"
      class={cn(
        // shimmer 自带 gradient 背景 + 横向位移动画，替代 animate-pulse 的简单透明度脉冲
        'animate-shimmer',
        props.rounded ? 'rounded-full' : 'rounded-lg',
        props.class,
      )}
      style={{
        width: props.width,
        height: props.height ?? '1rem',
      }}
    />
  );
}

export function CardSkeleton() {
  return (
    <div aria-hidden="true" class="p-5 rounded-xl bg-surface-elevated shadow-elevation-1 space-y-3">
      <Skeleton width="40%" height="1.25rem" />
      <Skeleton width="70%" />
      <Skeleton width="55%" />
    </div>
  );
}
