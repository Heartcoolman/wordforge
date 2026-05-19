import { cn } from '@/utils/cn';
import { Show } from 'solid-js';
import { Motion, DURATION, EASE } from '@/lib/motion';

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
          'relative inline-flex h-6 w-10 items-center rounded-full transition-colors duration-base ease-out-expo',
          props.checked ? 'bg-gradient-accent-strong shadow-elevation-1' : 'bg-surface-tertiary',
        )}
      >
        {/* thumb 用 Motion.span 由 spring 缓动驱动，比 CSS transition 更跟手 */}
        <Motion.span
          animate={{ x: props.checked ? 20 : 4 }}
          transition={{ duration: DURATION.base, easing: EASE.spring }}
          class="inline-block h-4 w-4 rounded-full bg-white shadow-elevation-1"
        />
      </button>
      <Show when={props.label}>
        <span class="text-sm text-content">{props.label}</span>
      </Show>
    </label>
  );
}
