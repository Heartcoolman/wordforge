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

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// 嵌套 Modal 引用计数，确保只有最后一个 Modal 关闭时才恢复 body overflow
let openModalCount = 0;

export function Modal(props: ModalProps) {
  let dialogRef: HTMLDivElement | undefined;
  let previouslyFocused: HTMLElement | null = null;

  createEffect(() => {
    if (props.open) {
      previouslyFocused = document.activeElement as HTMLElement | null;
      openModalCount++;
      document.body.style.overflow = 'hidden';

      // 焦点陷阱：聚焦第一个可交互元素
      requestAnimationFrame(() => {
        if (dialogRef) {
          const first = dialogRef.querySelector<HTMLElement>(FOCUSABLE_SELECTOR);
          first?.focus();
        }
      });

      const handler = (e: KeyboardEvent) => {
        if (e.key === 'Escape') {
          props.onClose();
          return;
        }

        // 焦点陷阱：Tab 键循环
        if (e.key === 'Tab' && dialogRef) {
          const focusable = Array.from(dialogRef.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR));
          if (focusable.length === 0) return;
          const first = focusable[0];
          const last = focusable[focusable.length - 1];

          if (e.shiftKey) {
            if (document.activeElement === first) {
              e.preventDefault();
              last.focus();
            }
          } else {
            if (document.activeElement === last) {
              e.preventDefault();
              first.focus();
            }
          }
        }
      };
      document.addEventListener('keydown', handler);
      onCleanup(() => {
        document.removeEventListener('keydown', handler);
        openModalCount--;
        if (openModalCount <= 0) {
          openModalCount = 0;
          document.body.style.overflow = '';
        }
        previouslyFocused?.focus();
      });
    } else {
      // 仅在没有其他打开的 Modal 时恢复 overflow
      if (openModalCount <= 0) {
        document.body.style.overflow = '';
      }
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
            ref={dialogRef}
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
                    aria-label="关闭"
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
