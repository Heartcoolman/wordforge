import { type JSX, splitProps, For, Show, createUniqueId } from 'solid-js';
import { cn } from '@/utils/cn';

interface SelectProps extends JSX.SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  error?: string;
  options: { value: string; label: string }[];
  placeholder?: string;
}

export function Select(props: SelectProps) {
  const [local, rest] = splitProps(props, ['label', 'error', 'options', 'placeholder', 'class', 'id', 'value']);
  const autoId = createUniqueId();
  const selectId = () => local.id ?? autoId;
  const errorId = () => `${selectId()}-error`;

  return (
    <div class="flex flex-col gap-1.5">
      <Show when={local.label}>
        <label for={selectId()} class="text-sm font-medium text-content-secondary">{local.label}</label>
      </Show>
      <div class="relative">
        <select
          id={selectId()}
          aria-invalid={local.error ? true : undefined}
          aria-describedby={local.error ? errorId() : undefined}
          value={local.value ?? ''}
          {...rest}
          class={cn(
            'w-full h-10 px-3 pr-8 rounded-lg text-sm bg-surface text-content appearance-none',
            'border border-border-hairline transition-[border-color,box-shadow,background-color] duration-fast ease-out-expo',
            'hover:border-border cursor-pointer',
            'focus-ring-soft focus:border-accent',
            'disabled:cursor-not-allowed disabled:opacity-60 disabled:bg-surface-secondary',
            local.error && 'border-error',
            local.class,
          )}
        >
          <Show when={local.placeholder}>
            <option value="" disabled selected={!local.value}>{local.placeholder}</option>
          </Show>
          <For each={local.options}>
            {(opt) => <option value={opt.value}>{opt.label}</option>}
          </For>
        </select>
        {/* 自定义箭头：用 currentColor 跟随 dark mode，替代背景图 */}
        <svg
          class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none w-4 h-4 text-content-secondary"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </div>
      <Show when={local.error}>
        <p id={errorId()} class="text-xs text-error" role="alert">{local.error}</p>
      </Show>
    </div>
  );
}
