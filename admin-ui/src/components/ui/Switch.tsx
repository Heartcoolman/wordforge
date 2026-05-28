import { cn } from '@/utils/cn';
import { Show, createUniqueId } from 'solid-js';

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
  class?: string;
}

export function Switch(props: SwitchProps) {
  const labelId = createUniqueId();

  // 注：外层从 <label> 改为 <div> —— label 包 button 不会触发 button click，
  // 且会让 aria-labelledby 语义重复；改用 div + 显式 aria-labelledby 关联 span label。
  // 轨道与 thumb 同走 CSS transition，时长一致，避免 Motion spring 与轨道 transition 时序错位。
  return (
    <div
      class={cn(
        'inline-flex items-center gap-2.5',
        props.disabled && 'opacity-50 pointer-events-none',
        props.class,
      )}
    >
      <button
        type="button"
        role="switch"
        aria-checked={props.checked}
        aria-labelledby={props.label ? labelId : undefined}
        aria-label={!props.label ? 'switch' : undefined}
        disabled={props.disabled}
        onClick={() => props.onChange(!props.checked)}
        class={cn(
          'relative inline-flex h-6 w-10 items-center rounded-full cursor-pointer',
          'transition-colors duration-base ease-out-expo',
          'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
          'disabled:cursor-not-allowed',
          props.checked ? 'bg-gradient-accent-strong shadow-elevation-1' : 'bg-surface-tertiary',
        )}
      >
        <span
          class={cn(
            'inline-block h-4 w-4 rounded-full bg-white shadow-elevation-1',
            'transition-transform duration-base ease-out-expo motion-reduce:transition-none',
          )}
          style={{ transform: `translateX(${props.checked ? 20 : 4}px)` }}
        />
      </button>
      <Show when={props.label}>
        <span
          id={labelId}
          class="text-sm text-content cursor-pointer select-none"
          onClick={() => { if (!props.disabled) props.onChange(!props.checked); }}
        >
          {props.label}
        </span>
      </Show>
    </div>
  );
}
