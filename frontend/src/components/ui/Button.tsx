import { type JSX, splitProps, Show } from 'solid-js';
import { cn } from '@/utils/cn';

const variants = {
  primary: 'bg-accent text-accent-content hover:bg-accent-hover shadow-sm',
  secondary: 'bg-surface-tertiary text-content hover:bg-border',
  outline: 'border border-border text-content hover:bg-surface-secondary',
  ghost: 'text-content hover:bg-surface-secondary',
  danger: 'bg-error text-white hover:opacity-90 shadow-sm',
  success: 'bg-success text-white hover:opacity-90 shadow-sm',
  warning: 'bg-warning text-white hover:opacity-90 shadow-sm',
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
        'inline-flex items-center justify-center font-medium transition-all duration-150',
        'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
        'disabled:opacity-50 disabled:pointer-events-none',
        'active:scale-[0.98]',
        'cursor-pointer',
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
