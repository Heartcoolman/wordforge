import { type JSX, Show, createEffect, on, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { cn } from '@/utils/cn';

interface DrawerProps {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: JSX.Element;
  /** 抽屉宽度,默认 360px;窄屏自动 min(100vw - 32px, width) */
  width?: number;
  /** footer 区域,通常放 cancel / save 按钮组 */
  footer?: JSX.Element;
  /** 点击 backdrop 是否关闭,默认 true */
  closeOnBackdrop?: boolean;
  /** 自定义右上额外操作(如 status chip / 删除按钮) */
  headerActions?: JSX.Element;
}

let openDrawerCount = 0;
let savedBodyOverflow: string | null = null;

/**
 * 右侧滑入抽屉,用于行展开详情(Feedback / WordbookCenter / UserManagement)。
 * 简化版相对 Modal:
 *  - 无 motion preset,直接 CSS transform 切换 translateX
 *  - Esc 关闭 + body overflow 锁
 *  - 多层嵌套用引用计数避免错乱
 *
 * 不提供完整焦点陷阱(用户使用键盘 Tab 不会限制在 Drawer 内),
 * 因为后台运维 GUI 多为鼠标操作,且 Drawer 内表单元素有限。
 */
export function Drawer(props: DrawerProps) {
  let panelRef: HTMLDivElement | undefined;
  let active = false;

  const teardown = () => {
    if (!active) return;
    active = false;
    openDrawerCount = Math.max(0, openDrawerCount - 1);
    if (openDrawerCount === 0 && savedBodyOverflow !== null) {
      document.body.style.overflow = savedBodyOverflow;
      savedBodyOverflow = null;
    }
    document.removeEventListener('keydown', onKeyDown);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      props.onClose();
    }
  };

  // open 切换:0→1 锁滚动,1→0 还原
  createEffect(on(() => props.open, (open) => {
    if (open) {
      if (active) return;
      active = true;
      if (openDrawerCount === 0) {
        savedBodyOverflow = document.body.style.overflow;
        document.body.style.overflow = 'hidden';
      }
      openDrawerCount++;
      document.addEventListener('keydown', onKeyDown);
    } else {
      teardown();
    }
  }));

  onCleanup(teardown);

  const handleBackdrop = () => {
    if (props.closeOnBackdrop === false) return;
    props.onClose();
  };

  return (
    <Show when={props.open}>
      <Portal>
        <div
          class="fixed inset-0 z-[200] flex justify-end animate-fade-in"
          role="dialog"
          aria-modal="true"
          aria-label={props.title ?? '抽屉'}
        >
          {/* backdrop */}
          <div
            class="absolute inset-0 bg-content/40 backdrop-blur-sm"
            onClick={handleBackdrop}
          />
          {/* panel */}
          <div
            ref={panelRef}
            class={cn(
              'relative h-full bg-surface-elevated shadow-elevation-4',
              'flex flex-col overflow-hidden',
              'animate-drawer-slide-in',
            )}
            style={{
              width: `min(${props.width ?? 360}px, calc(100vw - 32px))`,
            }}
          >
            {/* header */}
            <Show when={props.title || props.headerActions}>
              <header class="flex items-center justify-between gap-3 px-5 py-3.5 border-b border-border-hairline shrink-0">
                <h2 class="text-[15px] font-semibold text-content truncate">{props.title}</h2>
                <div class="flex items-center gap-1.5 shrink-0">
                  {props.headerActions}
                  <button
                    type="button"
                    class="size-7 rounded grid place-items-center text-content-tertiary hover:text-content hover:bg-surface-secondary focus-ring-soft"
                    aria-label="关闭"
                    onClick={() => props.onClose()}
                  >
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M18 6L6 18M6 6l12 12" />
                    </svg>
                  </button>
                </div>
              </header>
            </Show>
            {/* content */}
            <div class="flex-1 overflow-y-auto px-5 py-4">
              {props.children}
            </div>
            {/* footer */}
            <Show when={props.footer}>
              <footer class="px-5 py-3 border-t border-border-hairline bg-surface-secondary/40 shrink-0">
                {props.footer}
              </footer>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
