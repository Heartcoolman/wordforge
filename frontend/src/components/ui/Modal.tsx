import { type JSX, Show, createEffect, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { cn } from '@/utils/cn';

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: JSX.Element;
  size?: 'sm' | 'md' | 'lg' | 'xl';
  hideClose?: boolean;
}

const sizeMap = {
  sm: 'max-w-sm',
  md: 'max-w-md',
  lg: 'max-w-lg',
  xl: 'max-w-xl',
};

export function Modal(props: ModalProps) {
  createEffect(() => {
    if (props.open) {
      document.body.style.overflow = 'hidden';
      const handler = (e: KeyboardEvent) => {
        if (e.key === 'Escape') props.onClose();
      };
      document.addEventListener('keydown', handler);
      onCleanup(() => {
        document.removeEventListener('keydown', handler);
        document.body.style.overflow = '';
      });
    } else {
      document.body.style.overflow = '';
    }
  });

  return (
    <Show when={props.open}>
      <Portal>
        <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
          {/* Backdrop */}
          <div
            class="absolute inset-0 bg-black/50 animate-fade-in"
            onClick={props.onClose}
          />
          {/* Content */}
          <div
            role="dialog"
            aria-modal="true"
            aria-label={props.title}
            class={cn(
              'relative w-full bg-surface-elevated rounded-2xl shadow-xl animate-scale-in',
              'max-h-[85vh] overflow-y-auto',
              sizeMap[props.size ?? 'md'],
            )}
          >
            <Show when={props.title || !props.hideClose}>
              <div class="flex items-center justify-between px-6 pt-5 pb-2">
                <Show when={props.title}>
                  <h2 class="text-lg font-semibold text-content">{props.title}</h2>
                </Show>
                <Show when={!props.hideClose}>
                  <button
                    onClick={props.onClose}
                    class="p-1.5 rounded-lg text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors cursor-pointer"
                  >
                    <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </Show>
              </div>
            </Show>
            <div class="px-6 pb-6">{props.children}</div>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
