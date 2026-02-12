import { For, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { cn } from '@/utils/cn';
import { uiStore } from '@/stores/ui';
import type { ToastItem as ToastItemType } from '@/stores/ui';

const bgMap: Record<string, string> = {
  success: 'border-l-success',
  error: 'border-l-error',
  warning: 'border-l-warning',
  info: 'border-l-info',
};

function ToastIcon(props: { type: string }) {
  const paths: Record<string, string> = {
    success: 'M5 13l4 4L19 7',
    error: 'M6 18L18 6M6 6l12 12',
    warning: 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.034 16.5c-.77.833.192 2.5 1.732 2.5z',
    info: 'M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  };
  const colors: Record<string, string> = {
    success: 'text-success', error: 'text-error', warning: 'text-warning', info: 'text-info',
  };

  return (
    <svg class={cn('w-5 h-5', colors[props.type])} fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d={paths[props.type]} />
    </svg>
  );
}

function SingleToast(props: { toast: ToastItemType }) {
  return (
    <div class={cn(
      'flex items-start gap-3 p-4 rounded-lg shadow-lg animate-slide-in-right',
      'bg-surface-elevated border border-border border-l-4 min-w-[280px] max-w-[420px]',
      bgMap[props.toast.type],
    )}>
      <div class="flex-shrink-0 mt-0.5"><ToastIcon type={props.toast.type} /></div>
      <div class="flex-1 min-w-0">
        <p class="text-sm font-medium text-content">{props.toast.title}</p>
        <Show when={props.toast.message}>
          <p class="text-xs text-content-secondary mt-0.5">{props.toast.message}</p>
        </Show>
      </div>
      <button
        onClick={() => uiStore.removeToast(props.toast.id)}
        aria-label="关闭"
        class="flex-shrink-0 p-0.5 rounded text-content-tertiary hover:text-content cursor-pointer"
      >
        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}

export function Toaster() {
  return (
    <Portal>
      <div role="region" aria-live="polite" class="fixed top-4 right-4 z-[100] flex flex-col gap-2">
        <For each={uiStore.toasts()}>{(t) => <SingleToast toast={t} />}</For>
      </div>
    </Portal>
  );
}
