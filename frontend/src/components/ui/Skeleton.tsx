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
      class={cn(
        'animate-pulse bg-surface-tertiary',
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
    <div class="p-5 rounded-xl bg-surface-elevated shadow-md space-y-3">
      <Skeleton width="40%" height="1.25rem" />
      <Skeleton width="70%" />
      <Skeleton width="55%" />
    </div>
  );
}
