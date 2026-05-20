import { type JSX, splitProps, For, Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface SelectProps extends JSX.SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  error?: string;
  options: { value: string; label: string }[];
  placeholder?: string;
}

let selectIdCounter = 0;

export function Select(props: SelectProps) {
  const [local, rest] = splitProps(props, ['label', 'error', 'options', 'placeholder', 'class', 'id']);
  const selectId = local.id ?? `select-${++selectIdCounter}`;

  return (
    <div class="flex flex-col gap-1.5">
      <Show when={local.label}>
        <label for={selectId} class="text-sm font-medium text-content-secondary">{local.label}</label>
      </Show>
      <select
        id={selectId}
        {...rest}
        class={cn(
          'w-full h-10 px-3 rounded-lg text-sm bg-surface text-content appearance-none',
          'border border-border-hairline transition-[border-color,box-shadow,background-color] duration-fast ease-out-expo',
          'hover:border-border cursor-pointer',
          'focus-ring-soft focus:border-accent',
          'disabled:cursor-not-allowed disabled:opacity-60 disabled:bg-surface-secondary',
          'bg-[url("data:image/svg+xml,%3Csvg xmlns=\'http://www.w3.org/2000/svg\' width=\'16\' height=\'16\' viewBox=\'0 0 24 24\' fill=\'none\' stroke=\'currentColor\' stroke-width=\'2\'%3E%3Cpath d=\'m6 9 6 6 6-6\'/%3E%3C/svg%3E")] bg-no-repeat bg-[right_0.75rem_center]',
          local.error && 'border-error',
          local.class,
        )}
      >
        <Show when={local.placeholder}>
          <option value="" disabled>{local.placeholder}</option>
        </Show>
        <For each={local.options}>
          {(opt) => <option value={opt.value}>{opt.label}</option>}
        </For>
      </select>
      <Show when={local.error}>
        <p class="text-xs text-error">{local.error}</p>
      </Show>
    </div>
  );
}
