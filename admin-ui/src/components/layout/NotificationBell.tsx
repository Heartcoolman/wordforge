import { createSignal, For, Show, onCleanup, onMount } from 'solid-js';
import { cn } from '@/utils/cn';
import { formatRelativeTime } from '@/utils/formatters';
import { uiStore } from '@/stores/ui';
import { notificationsApi, type AdminAlert } from '@/api/notifications';

/** 未读计数轮询间隔。告警是低频信号,30s 足够;打开面板时即时拉全量。 */
const POLL_MS = 30_000;

const severityDot: Record<string, string> = {
  error: 'bg-error',
  warning: 'bg-warning',
  info: 'bg-accent',
};

/**
 * D1 admin 通知/告警收件箱:顶栏铃铛 + 未读角标 + 下拉面板。
 * 数据源是后端 system_alerts(AMAS 软拦截告警),经 /api/admin/notifications。
 * 打开面板拉全量(未读优先视觉区分),点条目标记已读并持久化 + 刷新角标。
 */
export function NotificationBell() {
  const [open, setOpen] = createSignal(false);
  const [unread, setUnread] = createSignal(0);
  const [items, setItems] = createSignal<AdminAlert[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [marking, setMarking] = createSignal<string | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | undefined;
  let rootRef: HTMLDivElement | undefined;

  const refreshCount = async () => {
    try {
      const inbox = await notificationsApi.list(true);
      setUnread(inbox.unreadCount);
    } catch { /* silent —— 角标拿不到不打扰 */ }
  };

  const loadInbox = async () => {
    setLoading(true);
    try {
      const inbox = await notificationsApi.list(false);
      setItems(inbox.items);
      setUnread(inbox.unreadCount);
    } catch {
      uiStore.toast.error('加载通知失败');
    } finally {
      setLoading(false);
    }
  };

  const toggle = () => {
    const next = !open();
    setOpen(next);
    if (next) loadInbox();
  };

  const markRead = async (alert: AdminAlert) => {
    if (alert.readAt || marking()) return;
    setMarking(alert.id);
    try {
      const res = await notificationsApi.markRead(alert.id);
      setUnread(res.unreadCount);
      const now = new Date().toISOString();
      setItems((prev) => prev.map((a) => (a.id === alert.id ? { ...a, readAt: now } : a)));
    } catch {
      uiStore.toast.error('标记已读失败');
    } finally {
      setMarking(null);
    }
  };

  const onDocClick = (e: MouseEvent) => {
    if (open() && rootRef && !rootRef.contains(e.target as Node)) setOpen(false);
  };
  const onKeydown = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && open()) setOpen(false);
  };

  onMount(() => {
    refreshCount();
    pollTimer = setInterval(refreshCount, POLL_MS);
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKeydown);
  });
  onCleanup(() => {
    if (pollTimer) clearInterval(pollTimer);
    document.removeEventListener('mousedown', onDocClick);
    document.removeEventListener('keydown', onKeydown);
  });

  return (
    <div ref={rootRef} class="relative">
      <button
        type="button"
        onClick={toggle}
        class="relative p-1.5 rounded-md text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer focus-ring-soft"
        aria-label={unread() > 0 ? `通知收件箱（${unread()} 条未读）` : '通知收件箱'}
        aria-haspopup="true"
        aria-expanded={open()}
        title="通知收件箱"
      >
        <svg class="w-[18px] h-[18px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9" />
        </svg>
        <Show when={unread() > 0}>
          <span class="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 grid place-items-center rounded-full bg-error text-white text-[10px] font-semibold leading-none">
            {unread() > 99 ? '99+' : unread()}
          </span>
        </Show>
      </button>

      <Show when={open()}>
        <div
          role="dialog"
          aria-label="通知收件箱"
          class="absolute right-0 mt-2 w-[340px] max-w-[calc(100vw-2rem)] rounded-lg border border-border-hairline bg-surface-elevated shadow-elevation-3 z-50 animate-fade-in overflow-hidden"
        >
          <div class="flex items-center justify-between px-4 py-2.5 border-b border-border-hairline">
            <span class="text-[13px] font-semibold text-content">通知</span>
            <Show when={unread() > 0}>
              <span class="text-[11px] text-content-tertiary">{unread()} 条未读</span>
            </Show>
          </div>

          <div class="max-h-[420px] overflow-y-auto">
            <Show
              when={!loading()}
              fallback={<div class="px-4 py-6 text-center text-[12px] text-content-tertiary">加载中…</div>}
            >
              <Show
                when={items().length > 0}
                fallback={
                  <div class="px-4 py-8 text-center text-[12px] text-content-tertiary">
                    暂无通知
                  </div>
                }
              >
                <For each={items()}>{(alert) => (
                  <button
                    type="button"
                    onClick={() => markRead(alert)}
                    class={cn(
                      'w-full text-left px-4 py-2.5 flex gap-2.5 border-b border-border-hairline/60 last:border-b-0',
                      'transition-colors duration-fast',
                      alert.readAt
                        ? 'opacity-60 hover:bg-surface-secondary/40 cursor-default'
                        : 'hover:bg-surface-secondary cursor-pointer',
                    )}
                  >
                    <span
                      aria-hidden="true"
                      class={cn(
                        'mt-1.5 size-2 shrink-0 rounded-full',
                        severityDot[alert.severity] ?? 'bg-content-tertiary',
                      )}
                    />
                    <span class="min-w-0 flex-1">
                      <span class="flex items-center gap-1.5">
                        <span class={cn('truncate text-[12.5px]', alert.readAt ? 'font-normal text-content-secondary' : 'font-semibold text-content')}>
                          {alert.title}
                        </span>
                        <Show when={alert.count > 1}>
                          <span class="shrink-0 text-[10px] px-1 rounded bg-surface-secondary text-content-tertiary">×{alert.count}</span>
                        </Show>
                      </span>
                      <span class="block mt-0.5 text-[11.5px] text-content-secondary line-clamp-2 break-words">{alert.message}</span>
                      <span class="block mt-0.5 text-[10.5px] text-content-tertiary">
                        {alert.source} · {formatRelativeTime(alert.lastSeenAt)}
                      </span>
                    </span>
                  </button>
                )}</For>
              </Show>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
}
