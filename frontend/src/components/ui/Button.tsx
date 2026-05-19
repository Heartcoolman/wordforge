import { type JSX, splitProps, Show } from 'solid-js';
import { cn } from '@/utils/cn';

const variants = {
  // Linear/Vercel 风：渐变主色 + layered shadow，hover 时多层阴影抬升 1px
  primary:
    'bg-gradient-accent-strong text-accent-content shadow-elevation-1 ' +
    'hover:shadow-elevation-2 hover:-translate-y-px active:translate-y-0 active:shadow-elevation-1',
  secondary:
    'bg-surface-tertiary text-content shadow-elevation-1 ' +
    'hover:bg-surface-secondary hover:shadow-elevation-2 hover:-translate-y-px active:translate-y-0',
  outline:
    'border border-border-hairline bg-surface text-content shadow-elevation-1 ' +
    'hover:bg-surface-secondary hover:border-border hover:-translate-y-px active:translate-y-0',
  ghost: 'text-content hover:bg-surface-secondary active:bg-surface-tertiary',
  danger:
    'bg-error text-white shadow-elevation-1 ' +
    'hover:bg-error/90 hover:shadow-elevation-2 hover:-translate-y-px active:translate-y-0',
  success:
    'bg-success text-white shadow-elevation-1 ' +
    'hover:bg-success/90 hover:shadow-elevation-2 hover:-translate-y-px active:translate-y-0',
  warning:
    'bg-warning text-white shadow-elevation-1 ' +
    'hover:bg-warning/90 hover:shadow-elevation-2 hover:-translate-y-px active:translate-y-0',
} as const;

const sizes = {
  xs: 'h-7 px-2 text-xs rounded-md gap-1',
  sm: 'h-8 px-3 text-sm rounded-md gap-1.5',
  md: 'h-9 px-4 text-sm rounded-lg gap-2',
  lg: 'h-10 px-5 text-base rounded-lg gap-2',
  xl: 'h-12 px-6 text-base rounded-xl gap-2.5',
} as const;

interface ButtonProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: keyof typeof variants;
  size?: keyof typeof sizes;
  loading?: boolean;
  icon?: JSX.Element;
  fullWidth?: boolean;
}

export function Button(props: ButtonProps) {
  const [local, rest] = splitProps(props, [
    'variant', 'size', 'loading', 'icon', 'fullWidth', 'class', 'children', 'disabled',
  ]);

  return (
    <button
      {...rest}
      disabled={local.disabled || local.loading}
      class={cn(
        'inline-flex items-center justify-center font-medium',
        // 仅过渡形变与阴影、背景；颜色由 variant 控制
        'transition-[transform,box-shadow,background-color,opacity] duration-fast ease-out-expo',
        'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
        'disabled:opacity-50 disabled:pointer-events-none disabled:hover:translate-y-0 disabled:hover:shadow-elevation-1',
        'cursor-pointer select-none',
        variants[local.variant ?? 'primary'],
        sizes[local.size ?? 'md'],
        local.fullWidth && 'w-full',
        local.class,
      )}
    >
      <Show when={local.loading}>
        <svg class="animate-spin h-4 w-4" viewBox="0 0 24 24" fill="none" role="status" aria-label="加载中">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
      </Show>
      <Show when={!local.loading && local.icon}>{local.icon}</Show>
      {local.children}
    </button>
  );
}
