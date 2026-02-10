import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type { AdminUser } from '@/types/admin';

export default function UserManagementPage() {
  const [users, setUsers] = createSignal<AdminUser[]>([]);
  const [loading, setLoading] = createSignal(true);

  async function load() {
    setLoading(true);
    try {
      const res = await adminApi.getUsers();
      setUsers(res);
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      setLoading(false);
    }
  }

  onMount(load);

  async function toggleBan(user: AdminUser) {
    try {
      if (user.isBanned) {
        await adminApi.unbanUser(user.id);
        uiStore.toast.success('已解封');
      } else {
        await adminApi.banUser(user.id);
        uiStore.toast.success('已封禁');
      }
      load();
    } catch (err: unknown) {
      uiStore.toast.error('操作失败', err instanceof Error ? err.message : '');
    }
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">用户管理</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <div class="overflow-x-auto rounded-xl border border-border">
          <table class="w-full text-sm">
            <thead>
              <tr class="bg-surface-secondary border-b border-border">
                <th class="px-4 py-3 text-left font-medium text-content-secondary">用户名</th>
                <th class="px-4 py-3 text-left font-medium text-content-secondary">邮箱</th>
                <th class="px-4 py-3 text-left font-medium text-content-secondary">状态</th>
                <th class="px-4 py-3 text-right font-medium text-content-secondary">操作</th>
              </tr>
            </thead>
            <tbody>
              <For each={users()}>
                {(user) => (
                  <tr class="border-b border-border last:border-b-0">
                    <td class="px-4 py-3 font-medium text-content">{user.username}</td>
                    <td class="px-4 py-3 text-content-secondary">{user.email}</td>
                    <td class="px-4 py-3">
                      <Badge variant={user.isBanned ? 'error' : 'success'}>
                        {user.isBanned ? '已封禁' : '正常'}
                      </Badge>
                    </td>
                    <td class="px-4 py-3 text-right">
                      <Button
                        size="xs"
                        variant={user.isBanned ? 'success' : 'danger'}
                        onClick={() => toggleBan(user)}
                      >
                        {user.isBanned ? '解封' : '封禁'}
                      </Button>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>
    </div>
  );
}
