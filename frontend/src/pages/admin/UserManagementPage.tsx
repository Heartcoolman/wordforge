import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Pagination } from '@/components/ui/Pagination';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type { AdminUser } from '@/types/admin';

/** 对邮箱进行部分脱敏，如 test@example.com -> t***@example.com */
function maskEmail(email: string): string {
  const atIndex = email.indexOf('@');
  if (atIndex <= 1) return email;
  return email[0] + '***' + email.slice(atIndex);
}

export default function UserManagementPage() {
  const [users, setUsers] = createSignal<AdminUser[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [loading, setLoading] = createSignal(true);
  const [confirmTarget, setConfirmTarget] = createSignal<AdminUser | null>(null);
  const pageSize = 20;

  async function load() {
    setLoading(true);
    try {
      const res = await adminApi.getUsers({ page: page(), perPage: pageSize });
      setUsers(res.data);
      setTotal(res.total);
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
    } finally {
      setConfirmTarget(null);
    }
  }

  function handleBanClick(user: AdminUser) {
    setConfirmTarget(user);
  }

  function confirmAction() {
    const target = confirmTarget();
    if (target) toggleBan(target);
  }

  function handlePageChange(nextPage: number) {
    setPage(nextPage);
    load();
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">用户管理</h1>

      {/* 确认弹窗 */}
      <Show when={confirmTarget()}>
        {(target) => (
          <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setConfirmTarget(null)}>
            <Card variant="elevated" class="max-w-sm mx-4" onClick={(e: MouseEvent) => e.stopPropagation()}>
              <h3 class="text-lg font-semibold text-content mb-2">
                {target().isBanned ? '确认解封' : '确认封禁'}
              </h3>
              <p class="text-sm text-content-secondary mb-4">
                确定要{target().isBanned ? '解封' : '封禁'}用户 <span class="font-medium text-content">{target().username}</span> 吗？
                {!target().isBanned && '封禁后该用户将无法登录，所有活跃会话将被撤销。'}
              </p>
              <div class="flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setConfirmTarget(null)}>取消</Button>
                <Button size="sm" variant={target().isBanned ? 'success' : 'danger'} onClick={confirmAction}>
                  {target().isBanned ? '确认解封' : '确认封禁'}
                </Button>
              </div>
            </Card>
          </div>
        )}
      </Show>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={users().length > 0} fallback={
          <Empty title="暂无用户" description="目前还没有注册用户" />
        }>
          <div class="space-y-3">
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
                        <td class="px-4 py-3 text-content-secondary">{maskEmail(user.email)}</td>
                        <td class="px-4 py-3">
                          <Badge variant={user.isBanned ? 'error' : 'success'}>
                            {user.isBanned ? '已封禁' : '正常'}
                          </Badge>
                        </td>
                        <td class="px-4 py-3 text-right">
                          <Button
                            size="xs"
                            variant={user.isBanned ? 'success' : 'danger'}
                            onClick={() => handleBanClick(user)}
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
            <div class="flex justify-between items-center">
              <Pagination page={page()} total={total()} pageSize={pageSize} onChange={handlePageChange} />
            </div>
          </div>
        </Show>
      </Show>
    </div>
  );
}
