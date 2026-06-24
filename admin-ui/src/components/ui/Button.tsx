import { type JSX, splitProps, Show } from 'solid-js';
import { cn } from '@/utils/cn';

const variants = {
  // 纯色主色 + layered shadow，hover 时多层阴影抬升 1px
  primary:
    'bg-accent text-accent-content shadow-elevation-1 ' +
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
    'variant', 'size', 'loading', 'icon', 'fullWidth', 'class', 'children', 'disabled', 'onClick',
  ]);

  // loading 时不用 disabled 属性，改用 aria-busy/aria-disabled 保留焦点和键盘流；
  // 在 onClick 内拦截避免点击触发。视觉禁用样式通过 aria-busy 选择器实现。
  const handleClick: JSX.EventHandlerUnion<HTMLButtonElement, MouseEvent> = (e) => {
    if (local.loading) {
      e.preventDefault();
      return;
    }
    const original = local.onClick;
    if (typeof original === 'function') original(e);
    else if (Array.isArray(original)) original[0](original[1], e);
  };

  return (
    <button
      {...rest}
      disabled={local.disabled}
      aria-busy={local.loading || undefined}
      aria-disabled={local.loading || undefined}
      onClick={handleClick}
      class={cn(
        'inline-flex items-center justify-center font-medium',
        // 仅过渡形变与阴影、背景；颜色由 variant 控制
        'transition-[transform,box-shadow,background-color,opacity] duration-fast ease-out-expo',
        'motion-reduce:transition-none motion-reduce:hover:translate-y-0',
        'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
        'disabled:opacity-50 disabled:pointer-events-none disabled:hover:translate-y-0 disabled:hover:shadow-elevation-1',
        'aria-busy:opacity-60 aria-busy:cursor-wait aria-busy:hover:translate-y-0 aria-busy:hover:shadow-elevation-1',
        'cursor-pointer select-none',
        variants[local.variant ?? 'primary'],
        sizes[local.size ?? 'md'],
        local.fullWidth && 'w-full',
        local.class,
      )}
    >
      {/* 统一 16x16 wrapper，loading 与 icon 切换时不产生 4px 抖动 */}
      <Show when={local.loading || local.icon}>
        <span class="inline-flex items-center justify-center w-4 h-4">
          <Show
            when={local.loading}
            fallback={local.icon}
          >
            <svg class="animate-spin h-4 w-4" viewBox="0 0 24 24" fill="none" role="status" aria-label="加载中">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </Show>
        </span>
      </Show>
      {local.children}
    </button>
  );
}
