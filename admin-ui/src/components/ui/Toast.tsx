import { For, Show, createSignal, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { cn } from '@/utils/cn';
import { uiStore } from '@/stores/ui';
import type { ToastItem as ToastItemType } from '@/stores/ui';
import { Motion, DURATION, EASE } from '@/lib/motion';

const bgMap: Record<string, string> = {
  success: 'border-l-success',
  error: 'border-l-error',
  warning: 'border-l-warning',
  info: 'border-l-info',
};

// 出场动画时长（毫秒）—与下面 Motion exit transition 对齐
const LEAVE_MS = 220;

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
  const [leaving, setLeaving] = createSignal(false);
  let leaveTimer: number | null = null;

  // 出场动画：点击关闭时先做出场，再从 store 剔除；
  // 注：store 的 addToast 自带 setTimeout 自动移除，那条路径会跳过出场动画，
  // 这条路径仅服务"用户主动关闭"。hover/focus 暂停需要在 store 层重构 timer（DEFERRED）。
  const triggerLeave = () => {
    if (leaving()) return;
    setLeaving(true);
    leaveTimer = window.setTimeout(() => uiStore.removeToast(props.toast.id), LEAVE_MS);
  };

  onCleanup(() => {
    if (leaveTimer !== null) clearTimeout(leaveTimer);
  });

  // role 区分：error/warning 用 alert（assertive），success/info 用 status（polite）
  const isAlert = () => props.toast.type === 'error' || props.toast.type === 'warning';

  return (
    <Motion.div
      initial={{ opacity: 0, x: 40 }}
      animate={leaving() ? { opacity: 0, x: 40 } : { opacity: 1, x: 0 }}
      transition={{ duration: leaving() ? LEAVE_MS / 1000 : DURATION.base, easing: EASE.outExpo }}
      role={isAlert() ? 'alert' : 'status'}
      aria-live={isAlert() ? 'assertive' : 'polite'}
      class={cn(
        'flex items-start gap-3 p-4 rounded-lg shadow-elevation-3',
        'bg-surface-elevated border border-border-hairline border-l-4 min-w-[280px] max-w-[420px]',
        bgMap[props.toast.type],
      )}
    >
      <div class="flex-shrink-0 mt-0.5"><ToastIcon type={props.toast.type} /></div>
      <div class="flex-1 min-w-0">
        <p class="text-sm font-medium text-content break-words">{props.toast.title}</p>
        <Show when={props.toast.message}>
          <p class="text-xs text-content-secondary mt-0.5 line-clamp-3 break-words" title={props.toast.message}>{props.toast.message}</p>
        </Show>
      </div>
      <button
        onClick={triggerLeave}
        aria-label="关闭"
        class="flex-shrink-0 p-0.5 rounded text-content-tertiary hover:text-content cursor-pointer transition-colors duration-fast"
      >
        <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </Motion.div>
  );
}

export function Toaster() {
  return (
    <Portal>
      {/* 容器只做布局；aria-live 由每条 toast 自身按 type 决定（assertive / polite） */}
      <div class="fixed top-4 right-4 z-[100] flex flex-col gap-2">
        {/* 注：solid-motionone v1 的 <Presence> 是 single-child 设计，与 <For> 组合时只渲染首个子节点；
            因此 toast 列表暂不使用 <Presence>，出场由 SingleToast 内部 leaving 信号驱动 */}
        <For each={uiStore.toasts()}>{(t) => <SingleToast toast={t} />}</For>
      </div>
    </Portal>
  );
}
