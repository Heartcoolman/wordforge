import { type JSX, splitProps, Show } from 'solid-js';
import { cn } from '@/utils/cn';

const variants = {
  default: 'bg-surface-tertiary text-content-secondary',
  accent: 'bg-accent-light text-accent',
  success: 'bg-success-light text-success',
  warning: 'bg-warning-light text-warning',
  error: 'bg-error-light text-error',
  info: 'bg-info-light text-info',
} as const;

const dotColorMap: Record<keyof typeof variants, string> = {
  default: 'bg-content-tertiary',
  accent: 'bg-accent',
  success: 'bg-success',
  warning: 'bg-warning',
  error: 'bg-error',
  info: 'bg-info',
};

interface BadgeProps extends JSX.HTMLAttributes<HTMLSpanElement> {
  variant?: keyof typeof variants;
  size?: 'sm' | 'md';
  /** 左侧色点，常用于 status badge（如在线/离线） */
  dot?: boolean;
  /** dot 是否带脉冲动画（适合实时状态） */
  pulse?: boolean;
}

export function Badge(props: BadgeProps) {
  const [local, rest] = splitProps(props, ['variant', 'size', 'dot', 'pulse', 'class', 'children']);
  const variant = () => local.variant ?? 'default';

  return (
    <span
      {...rest}
      class={cn(
        // chip 永远单行，避免被 flex 兄弟挤压换行（如 wordbook 卡片右上"已导入"）
        'inline-flex items-center font-medium rounded-full tabular-nums whitespace-nowrap shrink-0',
        local.size === 'sm' ? 'px-2 py-0.5 text-[10px] gap-1' : 'px-2.5 py-0.5 text-xs gap-1.5',
        variants[variant()],
        local.class,
      )}
    >
      <Show when={local.dot}>
        <span
          aria-hidden="true"
          class={cn(
            'inline-block rounded-full',
            local.size === 'sm' ? 'h-1.5 w-1.5' : 'h-2 w-2',
            dotColorMap[variant()],
            local.pulse && 'animate-ring-pulse',
          )}
          style={local.pulse ? { color: `var(--${variant() === 'default' ? 'content-tertiary' : variant()})` } : undefined}
        />
      </Show>
      {local.children}
    </span>
  );
}
