import { type JSX, splitProps } from 'solid-js';
import { cn } from '@/utils/cn';

const variants = {
  default: 'bg-surface-tertiary text-content-secondary',
  accent: 'bg-accent-light text-accent',
  success: 'bg-success-light text-success',
  warning: 'bg-warning-light text-warning',
  error: 'bg-error-light text-error',
  info: 'bg-info-light text-info',
} as const;

interface BadgeProps extends JSX.HTMLAttributes<HTMLSpanElement> {
  variant?: keyof typeof variants;
  size?: 'sm' | 'md';
}

export function Badge(props: BadgeProps) {
  const [local, rest] = splitProps(props, ['variant', 'size', 'class', 'children']);

  return (
    <span
      {...rest}
      class={cn(
        'inline-flex items-center font-medium rounded-full',
        local.size === 'sm' ? 'px-2 py-0.5 text-[10px]' : 'px-2.5 py-0.5 text-xs',
        variants[local.variant ?? 'default'],
        local.class,
      )}
    >
      {local.children}
    </span>
  );
}
