import { type JSX, splitProps, Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface InputProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  label?: string;
  error?: string;
  hint?: string;
  icon?: JSX.Element;
  rightIcon?: JSX.Element;
}

let inputIdCounter = 0;

export function Input(props: InputProps) {
  const [local, rest] = splitProps(props, ['label', 'error', 'hint', 'icon', 'rightIcon', 'class', 'id']);
  const inputId = local.id ?? `input-${++inputIdCounter}`;
  const errorId = `${inputId}-error`;

  return (
    <div class="flex flex-col gap-1.5">
      <Show when={local.label}>
        <label for={inputId} class="text-sm font-medium text-content-secondary">{local.label}</label>
      </Show>
      <div class="relative">
        <Show when={local.icon}>
          <div class="absolute left-3 top-1/2 -translate-y-1/2 text-content-tertiary">
            {local.icon}
          </div>
        </Show>
        <input
          id={inputId}
          aria-describedby={local.error ? errorId : undefined}
          aria-invalid={local.error ? true : undefined}
          {...rest}
          class={cn(
            'w-full h-10 px-3 rounded-lg text-sm bg-surface text-content',
            'border border-border-hairline transition-[border-color,box-shadow,background-color] duration-fast ease-out-expo',
            'placeholder:text-content-tertiary',
            'hover:border-border',
            'focus-ring-soft focus:border-accent',
            'disabled:cursor-not-allowed disabled:opacity-60 disabled:bg-surface-secondary',
            local.error && 'border-error focus:border-error',
            local.icon && 'pl-10',
            local.rightIcon && 'pr-10',
            local.class,
          )}
        />
        <Show when={local.rightIcon}>
          <div class="absolute right-3 top-1/2 -translate-y-1/2 text-content-tertiary">
            {local.rightIcon}
          </div>
        </Show>
      </div>
      <Show when={local.error}>
        <p id={errorId} class="text-xs text-error" role="alert">{local.error}</p>
      </Show>
      <Show when={local.hint && !local.error}>
        <p class="text-xs text-content-tertiary">{local.hint}</p>
      </Show>
    </div>
  );
}

interface TextAreaProps extends JSX.TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string;
  error?: string;
}

export function TextArea(props: TextAreaProps) {
  const [local, rest] = splitProps(props, ['label', 'error', 'class', 'id']);
  const textareaId = local.id ?? `textarea-${++inputIdCounter}`;

  return (
    <div class="flex flex-col gap-1.5">
      <Show when={local.label}>
        <label for={textareaId} class="text-sm font-medium text-content-secondary">{local.label}</label>
      </Show>
      <textarea
        id={textareaId}
        {...rest}
        class={cn(
          'w-full px-3 py-2 rounded-lg text-sm bg-surface text-content',
          'border border-border-hairline transition-[border-color,box-shadow,background-color] duration-fast ease-out-expo',
          'placeholder:text-content-tertiary',
          'hover:border-border',
          'focus-ring-soft focus:border-accent',
          'disabled:cursor-not-allowed disabled:opacity-60 disabled:bg-surface-secondary',
          'resize-y min-h-[80px]',
          local.error && 'border-error',
          local.class,
        )}
      />
      <Show when={local.error}>
        <p class="text-xs text-error">{local.error}</p>
      </Show>
    </div>
  );
}
