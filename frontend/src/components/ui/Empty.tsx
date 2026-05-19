import { Show, type JSX } from 'solid-js';
import { cn } from '@/utils/cn';

interface EmptyProps {
  icon?: JSX.Element;
  title?: string;
  description?: string;
  action?: JSX.Element;
  class?: string;
}

export function Empty(props: EmptyProps) {
  return (
    <div class={cn('flex flex-col items-center justify-center py-12 px-4 text-center animate-fade-in', props.class)}>
      <Show when={props.icon} fallback={
        <svg
          class="w-12 h-12 text-content-tertiary mb-4 animate-fade-in-up"
          fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"
        >
          <path stroke-linecap="round" stroke-linejoin="round" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
        </svg>
      }>
        <div class="mb-4 animate-fade-in-up">{props.icon}</div>
      </Show>
      <Show when={props.title}>
        <h3 class="text-base font-medium text-content-secondary mb-1 animate-fade-in-up" style={{ 'animation-delay': '80ms', 'animation-fill-mode': 'backwards' }}>{props.title}</h3>
      </Show>
      <Show when={props.description}>
        <p class="text-sm text-content-tertiary max-w-sm animate-fade-in-up" style={{ 'animation-delay': '160ms', 'animation-fill-mode': 'backwards' }}>{props.description}</p>
      </Show>
      <Show when={props.action}>
        <div class="mt-4 animate-fade-in-up" style={{ 'animation-delay': '240ms', 'animation-fill-mode': 'backwards' }}>{props.action}</div>
      </Show>
    </div>
  );
}
