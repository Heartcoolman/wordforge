import { cn } from '@/utils/cn';
import { Show } from 'solid-js';

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
  class?: string;
}

export function Switch(props: SwitchProps) {
  return (
    <label class={cn('inline-flex items-center gap-2.5 cursor-pointer', props.disabled && 'opacity-50 pointer-events-none', props.class)}>
      <button
        type="button"
        role="switch"
        aria-checked={props.checked}
        onClick={() => props.onChange(!props.checked)}
        class={cn(
          'relative inline-flex h-6 w-10 items-center rounded-full transition-colors duration-200',
          props.checked ? 'bg-accent' : 'bg-surface-tertiary',
        )}
      >
        <span
          class={cn(
            'inline-block h-4 w-4 rounded-full bg-white shadow-sm transition-transform duration-200',
            props.checked ? 'translate-x-5' : 'translate-x-1',
          )}
        />
      </button>
      <Show when={props.label}>
        <span class="text-sm text-content">{props.label}</span>
      </Show>
    </label>
  );
}
