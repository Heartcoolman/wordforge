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

type Variant = keyof typeof variants;

// dot + pulse 颜色统一在此映射，避免 inline style 与 class 不一致
const dotColorMap: Record<Variant, { base: string; pulse: string }> = {
  default: { base: 'bg-content-tertiary', pulse: 'shadow-[0_0_0_0_var(--content-tertiary)]' },
  accent: { base: 'bg-accent', pulse: 'shadow-[0_0_0_0_var(--accent)]' },
  success: { base: 'bg-success', pulse: 'shadow-[0_0_0_0_var(--success)]' },
  warning: { base: 'bg-warning', pulse: 'shadow-[0_0_0_0_var(--warning)]' },
  error: { base: 'bg-error', pulse: 'shadow-[0_0_0_0_var(--error)]' },
  info: { base: 'bg-info', pulse: 'shadow-[0_0_0_0_var(--info)]' },
};

interface BadgeProps extends JSX.HTMLAttributes<HTMLSpanElement> {
  variant?: Variant;
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
        // chip 永远单行，长内容用 truncate + max-w 兜底，避免被 flex 兄弟挤压换行
        'inline-flex items-center font-medium rounded-full tabular-nums whitespace-nowrap shrink-0 max-w-[10rem] truncate',
        local.size === 'sm' ? 'px-2 py-0.5 text-[10px] gap-1' : 'px-2.5 py-0.5 text-xs gap-1.5',
        variants[variant()],
        local.class,
      )}
    >
      <Show when={local.dot}>
        <span
          aria-hidden="true"
          class={cn(
            'inline-block rounded-full shrink-0',
            local.size === 'sm' ? 'h-1.5 w-1.5' : 'h-2 w-2',
            dotColorMap[variant()].base,
            local.pulse && 'animate-ring-pulse',
            local.pulse && dotColorMap[variant()].pulse,
          )}
        />
      </Show>
      {local.children}
    </span>
  );
}
