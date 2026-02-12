import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { notificationsApi } from '@/api/notifications';
import type { Notification } from '@/types/notification';
import { formatRelativeTime } from '@/utils/formatters';

export default function NotificationsPage() {
  const [items, setItems] = createSignal<Notification[]>([]);
  const [serverUnreadCount, setServerUnreadCount] = createSignal<number | null>(null);
  const [loading, setLoading] = createSignal(true);

  async function load() {
    setLoading(true);
    try {
      const [res, unread] = await Promise.all([
        notificationsApi.list({ limit: 50 }),
        notificationsApi.getUnreadCount().catch(() => null),
      ]);
      setItems(res ?? []);
      setServerUnreadCount(unread?.unreadCount ?? null);
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(load);

  async function markAllRead() {
    try {
      await notificationsApi.markAllRead();
      setItems((prev) => prev.map((n) => ({ ...n, read: true })));
      setServerUnreadCount(0);
      uiStore.toast.success('已全部标记已读');
    } catch {
      uiStore.toast.error('操作失败');
    }
  }

  async function markRead(id: string) {
    try {
      await notificationsApi.markRead(id);
      setItems((prev) => prev.map((n) => (n.id === id ? { ...n, read: true } : n)));
      setServerUnreadCount((prev) => (prev == null ? prev : Math.max(0, prev - 1)));
    } catch { /* ignore */ }
  }

  const localUnreadCount = () => items().filter((n) => !n.read).length;
  const unreadCount = () => serverUnreadCount() ?? localUnreadCount();

  return (
    <div class="max-w-2xl mx-auto space-y-6 animate-fade-in-up">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <h1 class="text-2xl font-bold text-content">通知</h1>
          <Show when={unreadCount() > 0}>
            <Badge variant="error">{unreadCount()} 未读</Badge>
          </Show>
        </div>
        <Show when={unreadCount() > 0}>
          <Button variant="ghost" size="sm" onClick={markAllRead}>全部已读</Button>
        </Show>
      </div>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={items().length > 0} fallback={<Empty title="暂无通知" />}>
          <div class="space-y-2">
            <For each={items()}>
              {(n) => (
                <Card
                  variant={n.read ? 'outlined' : 'filled'}
                  padding="sm"
                  hover
                  onClick={() => !n.read && markRead(n.id)}
                  class={!n.read ? 'border-l-4 border-l-accent' : ''}
                  role={!n.read ? 'button' : undefined}
                  tabIndex={!n.read ? 0 : undefined}
                  onKeyDown={(e: KeyboardEvent) => {
                    if (!n.read && (e.key === 'Enter' || e.key === ' ')) {
                      e.preventDefault();
                      markRead(n.id);
                    }
                  }}
                >
                  <div class="flex justify-between">
                    <div>
                      <p class="font-medium text-content text-sm">{n.title}</p>
                      <p class="text-xs text-content-secondary mt-0.5">{n.message}</p>
                    </div>
                    <span class="text-xs text-content-tertiary flex-shrink-0 ml-4">
                      {formatRelativeTime(n.createdAt)}
                    </span>
                  </div>
                </Card>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
}
