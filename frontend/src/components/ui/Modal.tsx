import { type JSX, Show, createEffect, on, onCleanup, splitProps } from 'solid-js';
import { Portal } from 'solid-js/web';
import { cn } from '@/utils/cn';
import { Motion, Presence, modalPreset } from '@/lib/motion';

interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: JSX.Element;
  size?: 'sm' | 'md' | 'lg' | 'xl';
  hideClose?: boolean;
  /** 点击遮罩是否关闭，默认 true；danger/loading 场景建议关掉 */
  closeOnBackdrop?: boolean;
}

const sizeMap = {
  sm: 'max-w-sm',
  md: 'max-w-md',
  lg: 'max-w-lg',
  xl: 'max-w-xl',
};

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

// 嵌套 Modal 引用计数 — 只有计数 0→1 时保存原 overflow，1→0 时还原；
// 中间嵌套不再覆盖保存值，避免多层弹窗错乱。
let openModalCount = 0;
let savedBodyOverflow: string | null = null;

// design tokens: z-modal=50, z-toast=100, z-dropdown=40

export function Modal(props: ModalProps) {
  const [local] = splitProps(props, [
    'open', 'onClose', 'title', 'children', 'size', 'hideClose', 'closeOnBackdrop',
  ]);

  let dialogRef: HTMLDivElement | undefined;
  let previouslyFocused: HTMLElement | null = null;
  let keydownHandler: ((e: KeyboardEvent) => void) | null = null;
  let active = false; // 本实例是否已计入 openModalCount，避免重复 ++/--

  const teardown = () => {
    if (!active) return;
    active = false;
    if (keydownHandler) {
      document.removeEventListener('keydown', keydownHandler);
      keydownHandler = null;
    }
    openModalCount = Math.max(0, openModalCount - 1);
    if (openModalCount === 0 && savedBodyOverflow !== null) {
      document.body.style.overflow = savedBodyOverflow;
      savedBodyOverflow = null;
    }
    previouslyFocused?.focus();
    previouslyFocused = null;
  };

  // 用 on(props.open) 精确驱动 open/close 切换，避免 createEffect 多次重入导致计数失衡
  createEffect(on(() => local.open, (isOpen, prev) => {
    if (isOpen === prev) return;
    if (isOpen) {
      if (active) return; // 防御重复触发
      active = true;
      previouslyFocused = document.activeElement as HTMLElement | null;
      if (openModalCount === 0) {
        savedBodyOverflow = document.body.style.overflow;
        document.body.style.overflow = 'hidden';
      }
      openModalCount++;

      // 焦点陷阱：聚焦第一个非关闭按钮的可交互元素；兜底落到容器自身
      requestAnimationFrame(() => {
        if (!dialogRef) return;
        const first = dialogRef.querySelector<HTMLElement>(
          FOCUSABLE_SELECTOR + ':not([data-modal-close="true"])',
        );
        (first ?? dialogRef).focus();
      });

      keydownHandler = (e: KeyboardEvent) => {
        if (e.key === 'Escape') {
          local.onClose();
          return;
        }
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
      document.addEventListener('keydown', keydownHandler);
    } else {
      teardown();
    }
  }));

  // 组件被卸载时兜底，避免计数泄漏
  onCleanup(teardown);

  const showHeader = () => Boolean(local.title) || !local.hideClose;
  // hideClose 且无其他右侧元素时，header 不需要 justify-between
  const headerJustify = () => (local.hideClose ? '' : 'justify-between');

  return (
    <Portal>
      <Presence exitBeforeEnter>
        <Show when={local.open}>
          <div class="fixed inset-0 z-50 flex items-center justify-center p-4">
            {/* Backdrop — 独立 Motion 以独立淡入淡出 */}
            <Motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.18 }}
              class="absolute inset-0 bg-black/40 backdrop-blur-sm"
              onClick={() => {
                if (local.closeOnBackdrop === false) return;
                local.onClose();
              }}
            />
            {/* Content — 由 modalPreset 驱动进出场 */}
            <Motion.div
              {...modalPreset}
              ref={(el: HTMLDivElement) => (dialogRef = el)}
              role="dialog"
              aria-modal="true"
              aria-label={local.title}
              tabindex="-1"
              class={cn(
                'relative w-full bg-surface-elevated rounded-2xl shadow-elevation-4 border border-border-hairline',
                // dialog 本身做滚动容器，让 header 的 sticky 生效
                'max-h-[85vh] overflow-y-auto',
                sizeMap[local.size ?? 'md'],
              )}
            >
              <Show when={showHeader()}>
                <div class={cn(
                  'flex items-center gap-2 px-6 pt-5 pb-2 sticky top-0 bg-surface-elevated z-10 rounded-t-2xl',
                  headerJustify(),
                )}>
                  <Show when={local.title}>
                    <h2 class="text-lg font-semibold text-content truncate min-w-0 flex-1 pr-2" title={local.title}>{local.title}</h2>
                  </Show>
                  <Show when={!local.hideClose}>
                    <button
                      onClick={local.onClose}
                      aria-label="关闭"
                      data-modal-close="true"
                      class="p-1.5 rounded-lg text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer"
                    >
                      <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </Show>
                </div>
              </Show>
              <div class="px-6 pb-6">{local.children}</div>
            </Motion.div>
          </div>
        </Show>
      </Presence>
    </Portal>
  );
}
