import { createSignal, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Input } from '@/components/ui/Input';
import { Pagination } from '@/components/ui/Pagination';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type { AdminUser } from '@/types/admin';

// 重置模式选择卡片：复用 Card interactive variant 的标准 hover/elevation
function ResetModeCard(props: { title: string; description: string; onClick: () => void }) {
  return (
    <Card variant="interactive" padding="md" onClick={props.onClick}>
      <p class="font-medium text-content">{props.title}</p>
      <p class="text-caption mt-1">{props.description}</p>
    </Card>
  );
}

/** 对邮箱进行部分脱敏，如 test@example.com -> t***@example.com */
function maskEmail(email: string): string {
  const atIndex = email.indexOf('@');
  if (atIndex <= 1) return email;
  return email[0] + '***' + email.slice(atIndex);
}

type ResetMode = 'choose' | 'direct' | 'key-result';

export default function UserManagementPage() {
  const [users, setUsers] = createSignal<AdminUser[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  const [loading, setLoading] = createSignal(true);
  const [confirmTarget, setConfirmTarget] = createSignal<AdminUser | null>(null);
  // 行级 busy 标记：防止用户在某一行操作进行中重复点击（AdminUser.id 是 string）
  const [busyUserId, setBusyUserId] = createSignal<string | null>(null);

  // 密码重置 Modal 状态
  const [resetTarget, setResetTarget] = createSignal<AdminUser | null>(null);
  const [resetMode, setResetMode] = createSignal<ResetMode>('choose');
  const [directPassword, setDirectPassword] = createSignal('');
  const [directConfirm, setDirectConfirm] = createSignal('');
  const [directError, setDirectError] = createSignal('');
  const [directLoading, setDirectLoading] = createSignal(false);
  const [generatedKey, setGeneratedKey] = createSignal('');
  const [keyLoading, setKeyLoading] = createSignal(false);
  const pageSize = 20;

  function closeResetModal() {
    setResetTarget(null);
    setResetMode('choose');
    setDirectPassword('');
    setDirectConfirm('');
    setDirectError('');
    setDirectLoading(false);
    setGeneratedKey('');
    setKeyLoading(false);
  }

  async function load(p?: number) {
    setLoading(true);
    try {
      const res = await adminApi.getUsers({ page: p ?? page(), perPage: pageSize });
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
    setBusyUserId(user.id);
    try {
      if (user.isBanned) {
        await adminApi.unbanUser(user.id);
        uiStore.toast.success(`已解封 ${user.username}`);
      } else {
        await adminApi.banUser(user.id);
        uiStore.toast.success(`已封禁 ${user.username}`);
      }
      load();
    } catch (err: unknown) {
      uiStore.toast.error(`${user.isBanned ? '解封' : '封禁'} ${user.username} 失败`, err instanceof Error ? err.message : '');
    } finally {
      setConfirmTarget(null);
      setBusyUserId(null);
    }
  }

  async function handleDirectReset(e: Event) {
    e.preventDefault();
    const target = resetTarget();
    if (!target) return;
    if (!directPassword()) {
      setDirectError('请输入新密码');
      return;
    }
    if (directPassword() !== directConfirm()) {
      setDirectError('两次密码输入不一致');
      return;
    }
    setDirectLoading(true);
    setDirectError('');
    try {
      await adminApi.setUserPassword(target.id, directPassword());
      uiStore.toast.success(`已重置 ${target.username} 的密码`);
      closeResetModal();
    } catch (err: unknown) {
      setDirectError(err instanceof Error ? err.message : '密码重置失败');
    } finally {
      setDirectLoading(false);
    }
  }

  async function handleGenerateKey() {
    const target = resetTarget();
    if (!target) return;
    setKeyLoading(true);
    try {
      const res = await adminApi.resetUserPassword(target.id);
      setGeneratedKey(res.resetKey);
    } catch (err: unknown) {
      uiStore.toast.error('生成密钥失败', err instanceof Error ? err.message : '');
    } finally {
      setKeyLoading(false);
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
    load(nextPage);
  }

  return (
    <div class="space-y-6">

      {/* 确认弹窗 */}
      <Show when={confirmTarget()}>
        {(target) => (
          <ConfirmDialog
            open={true}
            title={target().isBanned ? '确认解封' : '确认封禁'}
            message={
              <>
                确定要{target().isBanned ? '解封' : '封禁'}用户 <span class="font-medium text-content">{target().username}</span> 吗？
                {!target().isBanned && '封禁后该用户将无法登录，所有活跃会话将被撤销。'}
              </>
            }
            confirmText={target().isBanned ? '确认解封' : '确认封禁'}
            variant={target().isBanned ? 'success' : 'danger'}
            onConfirm={confirmAction}
            onCancel={() => setConfirmTarget(null)}
          />
        )}
      </Show>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={users().length > 0} fallback={
          <Empty title="暂无用户" description="目前还没有注册用户" />
        }>
          <div class="space-y-3">
            <div class="overflow-x-auto rounded-xl border border-border-hairline shadow-elevation-1">
              <table class="w-full text-sm">
                <thead>
                  <tr class="bg-surface-secondary/60 backdrop-blur-sm border-b border-border-hairline">
                    <th class="px-4 py-3 text-left text-caption uppercase tracking-wide font-medium text-content-secondary">用户名</th>
                    <th class="px-4 py-3 text-left text-caption uppercase tracking-wide font-medium text-content-secondary">邮箱</th>
                    <th class="px-4 py-3 text-left text-caption uppercase tracking-wide font-medium text-content-secondary">状态</th>
                    <th class="px-4 py-3 text-right text-caption uppercase tracking-wide font-medium text-content-secondary">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={users()}>
                    {(user) => (
                      <tr class="border-b border-border-hairline last:border-b-0 hover:bg-accent-light/40 transition-colors duration-fast ease-out-expo">
                        <td class="px-4 py-3 font-medium text-content">{user.username}</td>
                        <td class="px-4 py-3 text-content-secondary">{maskEmail(user.email)}</td>
                        <td class="px-4 py-3">
                          <Badge variant={user.isBanned ? 'error' : 'success'} dot>
                            {user.isBanned ? '已封禁' : '正常'}
                          </Badge>
                        </td>
                        <td class="px-4 py-3">
                          <div class="flex items-center justify-end gap-2">
                            <Button
                              size="xs"
                              variant="outline"
                              disabled={busyUserId() === user.id}
                              onClick={() => { closeResetModal(); setResetTarget(user); }}
                            >
                              重置密码
                            </Button>
                            <Button
                              size="xs"
                              variant={user.isBanned ? 'success' : 'danger'}
                              loading={busyUserId() === user.id}
                              disabled={busyUserId() !== null && busyUserId() !== user.id}
                              onClick={() => handleBanClick(user)}
                            >
                              {user.isBanned ? '解封' : '封禁'}
                            </Button>
                          </div>
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

      {/* 密码重置 Modal */}
      <Modal open={!!resetTarget()} onClose={closeResetModal} title={`重置密码 - ${resetTarget()?.username ?? ''}`} size="sm">
        {/* 选择模式 */}
        <Show when={resetMode() === 'choose'}>
          <div class="space-y-3 mt-2">
            <ResetModeCard
              title="直接重置密码"
              description="由管理员设定新密码，用户现有会话将被注销"
              onClick={() => setResetMode('direct')}
            />
            <ResetModeCard
              title="生成重置密钥"
              description="生成一次性密钥发送给用户，由用户自行修改密码"
              onClick={() => { setResetMode('key-result'); handleGenerateKey(); }}
            />
          </div>
        </Show>

        {/* 直接重置 */}
        <Show when={resetMode() === 'direct'}>
          <form onSubmit={handleDirectReset} class="space-y-4 mt-2">
            <Input
              label="新密码"
              type="password"
              placeholder="输入新密码"
              value={directPassword()}
              disabled={directLoading()}
              onInput={(e) => setDirectPassword(e.currentTarget.value)}
            />
            <Input
              label="确认密码"
              type="password"
              placeholder="再次输入新密码"
              value={directConfirm()}
              disabled={directLoading()}
              onInput={(e) => setDirectConfirm(e.currentTarget.value)}
            />
            <Show when={directError()}><p class="text-sm text-error text-center">{directError()}</p></Show>
            <div class="flex justify-end gap-2 pt-2">
              <Button variant="ghost" disabled={directLoading()} onClick={() => { setResetMode('choose'); setDirectError(''); setDirectPassword(''); setDirectConfirm(''); }}>返回</Button>
              <Button type="submit" loading={directLoading()}>确认重置</Button>
            </div>
          </form>
        </Show>

        {/* 密钥结果 */}
        <Show when={resetMode() === 'key-result'}>
          <div class="space-y-4 mt-2">
            <Show when={keyLoading()}>
              <div class="flex justify-center py-6"><Spinner /></div>
            </Show>
            <Show when={generatedKey()}>
              <p class="text-sm text-content-secondary">请将以下密钥发送给用户：</p>
              <div class="flex items-center gap-2 p-3 rounded-lg bg-surface-secondary border border-border min-w-0">
                <code class="flex-1 min-w-0 text-sm font-mono text-content break-all select-all">{generatedKey()}</code>
                <Button
                  size="xs"
                  variant="outline"
                  onClick={async () => {
                    try {
                      await navigator.clipboard.writeText(generatedKey());
                      uiStore.toast.success('已复制到剪贴板');
                    } catch {
                      uiStore.toast.error('复制失败', '请手动选择并复制');
                    }
                  }}
                >
                  复制
                </Button>
              </div>
              <p class="text-xs text-content-tertiary">密钥有效期 24 小时，使用后自动失效</p>
            </Show>
            <div class="flex justify-end gap-2 pt-2">
              <Show when={!keyLoading()}>
                <Button variant="ghost" onClick={() => { setResetMode('choose'); setGeneratedKey(''); }}>返回</Button>
                <Button onClick={closeResetModal}>关闭</Button>
              </Show>
            </div>
          </div>
        </Show>
      </Modal>
    </div>
  );
}
