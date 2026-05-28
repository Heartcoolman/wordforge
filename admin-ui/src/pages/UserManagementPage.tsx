import { createSignal, createMemo, createEffect, createResource, Show, For, onMount, onCleanup } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { HeroCard } from '@/components/ui/HeroCard';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Drawer } from '@/components/ui/Drawer';
import { Input } from '@/components/ui/Input';
import { Pagination } from '@/components/ui/Pagination';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type { AdminUser } from '@/types/admin';

// 会话状态短中文
const SESSION_STATUS_LABEL: Record<string, string> = {
  active: '进行中',
  completed: '已完成',
  abandoned: '已放弃',
  paused: '已暂停',
};
const SESSION_STATUS_VARIANT: Record<string, 'info' | 'success' | 'default' | 'warning'> = {
  active: 'info',
  completed: 'success',
  abandoned: 'default',
  paused: 'warning',
};

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
  // 首次进入用 loading（整页 Spinner），翻页用 pageChanging（覆盖层不闪表格）
  const [loading, setLoading] = createSignal(true);
  const [pageChanging, setPageChanging] = createSignal(false);
  const [confirmTarget, setConfirmTarget] = createSignal<AdminUser | null>(null);
  // 行级 busy 标记：防止用户在某一行操作进行中重复点击（AdminUser.id 是 string）
  const [busyUserId, setBusyUserId] = createSignal<string | null>(null);

  // 搜索过滤（前端 filter；列表 API 暂无 query 参数）
  const [searchQuery, setSearchQuery] = createSignal('');
  const [debouncedQuery, setDebouncedQuery] = createSignal('');
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  function onSearchInput(v: string) {
    setSearchQuery(v);
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => setDebouncedQuery(v.trim().toLowerCase()), 300);
  }
  onCleanup(() => { if (searchTimer) clearTimeout(searchTimer); });
  const filteredUsers = createMemo(() => {
    const q = debouncedQuery();
    if (!q) return users();
    return users().filter((u) => u.username.toLowerCase().includes(q));
  });

  // 密码重置 Modal 状态
  const [resetTarget, setResetTarget] = createSignal<AdminUser | null>(null);
  const [resetMode, setResetMode] = createSignal<ResetMode>('choose');
  const [directPassword, setDirectPassword] = createSignal('');
  const [directConfirm, setDirectConfirm] = createSignal('');
  const [directError, setDirectError] = createSignal('');
  const [directLoading, setDirectLoading] = createSignal(false);
  const [generatedKey, setGeneratedKey] = createSignal('');
  const [keyLoading, setKeyLoading] = createSignal(false);
  // Modal title 缓存：避免关闭瞬间 resetTarget 置 null 导致 title 闪空
  const [resetTitle, setResetTitle] = createSignal('重置密码');
  createEffect(() => {
    const u = resetTarget()?.username;
    if (u) setResetTitle(`重置密码 - ${u}`);
  });
  // 关闭 key-result 时若密钥未复制，弹二次确认
  const [showCloseKeyConfirm, setShowCloseKeyConfirm] = createSignal(false);
  const pageSize = 20;

  // 多选 checkbox + 详情 Drawer + 批量操作
  const [selectedIds, setSelectedIds] = createSignal<Set<string>>(new Set());
  const [detailUser, setDetailUser] = createSignal<AdminUser | null>(null);
  const [bulkBusy, setBulkBusy] = createSignal<'ban' | 'unban' | null>(null);

  // m022:Drawer 打开时拉 profile + sessions
  const [profile] = createResource(detailUser, async (u) => (u ? adminApi.userProfile(u.id) : null));
  const [sessionsResp] = createResource(detailUser, async (u) => (u ? adminApi.userSessions(u.id, 20) : null));
  const sessions = () => sessionsResp()?.sessions ?? null;
  const allSelected = createMemo(() => {
    const list = filteredUsers();
    return list.length > 0 && list.every((u) => selectedIds().has(u.id));
  });
  function toggleSelect(id: string) {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }
  function toggleSelectAll() {
    const list = filteredUsers();
    setSelectedIds((prev) => {
      if (list.every((u) => prev.has(u.id))) return new Set<string>();
      const next = new Set(prev);
      list.forEach((u) => next.add(u.id));
      return next;
    });
  }
  async function bulkToggleBan(action: 'ban' | 'unban') {
    const ids = Array.from(selectedIds());
    if (ids.length === 0) return;
    setBulkBusy(action);
    try {
      // m022:走 atomic 批量端点,后端单次返回 succeeded/failed 计数 + 失败明细
      const res = action === 'ban'
        ? await adminApi.usersBulkBan(ids)
        : await adminApi.usersBulkUnban(ids);
      if (res.failed === 0) {
        uiStore.toast.success(`${action === 'ban' ? '批量封禁' : '批量解封'}成功 (${res.succeeded})`);
      } else {
        uiStore.toast.error(`部分失败:${res.succeeded} 成功 / ${res.failed} 失败`);
      }
    } catch (e) {
      uiStore.toast.error('批量操作失败', e instanceof Error ? e.message : '');
    } finally {
      setSelectedIds(new Set<string>());
      setBulkBusy(null);
      void load();
    }
  }
  function exportUsersCsv() {
    const rows: string[][] = [['id', 'username', 'email', 'status']];
    const targetUsers = selectedIds().size > 0
      ? users().filter((u) => selectedIds().has(u.id))
      : filteredUsers();
    targetUsers.forEach((u) => {
      rows.push([u.id, u.username, maskEmail(u.email), u.isBanned ? 'banned' : 'active']);
    });
    if (rows.length === 1) {
      uiStore.toast.warning('当前列表为空');
      return;
    }
    const csv = rows
      .map((r) => r.map((c) => (c.includes(',') || c.includes('"') ? `"${c.replace(/"/g, '""')}"` : c)).join(','))
      .join('\n');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `wordforge-users-${new Date().toISOString().slice(0, 10)}.csv`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    uiStore.toast.success(`已导出 ${targetUsers.length} 条用户`);
  }

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

  // 翻页时使用 pageChanging（覆盖层）而非 loading（整页 Spinner），避免表格闪烁
  async function load(p?: number, opts?: { paging?: boolean }) {
    const paging = opts?.paging ?? false;
    if (paging) setPageChanging(true); else setLoading(true);
    try {
      const res = await adminApi.getUsers({ page: p ?? page(), perPage: pageSize });
      setUsers(res.data);
      setTotal(res.total);
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      if (paging) setPageChanging(false); else setLoading(false);
    }
  }

  onMount(() => { void load(); });

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
    if (keyLoading()) return; // 防抖：重复点击直接 return
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
    void load(nextPage, { paging: true });
  }

  // 关闭 reset Modal：若处于 key-result 且已生成 key，需要二次确认（密钥不可恢复）
  function handleCloseResetModal() {
    if (resetMode() === 'key-result' && generatedKey()) {
      setShowCloseKeyConfirm(true);
      return;
    }
    closeResetModal();
  }

  return (
    <div class="space-y-6">
      <HeroCard
        eyebrow="批量 + 行抽屉"
        eyebrowVariant="info"
        title="用户管理"
        desc="高级过滤、批量禁用 / 重置 / 导出，行内抽屉查看答题档案 + 设备会话。"
        meta={[
          { value: users().length, label: '当前列表' },
        ]}
      />

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
            {/* 顶部搜索 toolbar：debounce 300ms 后过滤当前页 */}
            <div class="flex items-center gap-2">
              <div class="w-full max-w-xs">
                <Input
                  placeholder="搜索用户名…"
                  value={searchQuery()}
                  onInput={(e) => onSearchInput(e.currentTarget.value)}
                />
              </div>
              <Show when={debouncedQuery()}>
                <span class="text-xs text-content-tertiary">
                  当前页匹配 {filteredUsers().length}/{users().length}
                </span>
              </Show>
            </div>
            {/* 批量操作工具栏:选中 > 0 时显示批量按钮,否则显示'导出 CSV' */}
            <div class="flex flex-wrap items-center gap-2 text-[12.5px]">
              <Show
                when={selectedIds().size > 0}
                fallback={
                  <>
                    <span class="text-content-tertiary">未选中</span>
                    <span class="flex-1" />
                    <Button size="xs" variant="ghost" onClick={exportUsersCsv}>
                      导出当前列表 CSV
                    </Button>
                  </>
                }
              >
                <span class="text-content">已选 <span class="font-mono tabular-nums">{selectedIds().size}</span> 项</span>
                <Button size="xs" variant="danger" loading={bulkBusy() === 'ban'} onClick={() => bulkToggleBan('ban')}>批量封禁</Button>
                <Button size="xs" variant="success" loading={bulkBusy() === 'unban'} onClick={() => bulkToggleBan('unban')}>批量解封</Button>
                <Button size="xs" variant="ghost" onClick={exportUsersCsv}>导出选中 CSV</Button>
                <Button size="xs" variant="ghost" onClick={() => setSelectedIds(new Set<string>())}>清空选择</Button>
              </Show>
            </div>

            <div class="relative overflow-x-auto rounded-xl border border-border-hairline shadow-elevation-1">
              <table class="w-full text-sm">
                <thead>
                  <tr class="bg-surface-secondary/60 backdrop-blur-sm border-b border-border-hairline">
                    <th class="px-3 py-3 w-9 text-center">
                      <input
                        type="checkbox"
                        aria-label="全选 / 全不选"
                        checked={allSelected()}
                        onChange={toggleSelectAll}
                        class="size-3.5 cursor-pointer accent-accent"
                      />
                    </th>
                    <th class="px-4 py-3 text-left text-caption uppercase tracking-wide font-medium text-content-secondary">用户名</th>
                    <th class="px-4 py-3 text-left text-caption uppercase tracking-wide font-medium text-content-secondary">邮箱</th>
                    <th class="px-4 py-3 text-left text-caption uppercase tracking-wide font-medium text-content-secondary">状态</th>
                    <th class="px-4 py-3 text-right text-caption uppercase tracking-wide font-medium text-content-secondary">操作</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={filteredUsers()}>
                    {(user) => (
                      <tr
                        class="border-b border-border-hairline last:border-b-0 hover:bg-accent-light/40 transition-colors duration-fast ease-out-expo cursor-pointer"
                        onClick={(e) => {
                          // 排除点击 checkbox / button 时打开 drawer,避免误触
                          const tgt = e.target as HTMLElement;
                          if (tgt.closest('input,button')) return;
                          setDetailUser(user);
                        }}
                      >
                        <td class="px-3 py-3 text-center" onClick={(e) => e.stopPropagation()}>
                          <input
                            type="checkbox"
                            aria-label={`选中 ${user.username}`}
                            checked={selectedIds().has(user.id)}
                            onChange={() => toggleSelect(user.id)}
                            class="size-3.5 cursor-pointer accent-accent"
                          />
                        </td>
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
              {/* 翻页时覆盖层：保留旧数据，避免整表替换为 Spinner 的 flicker */}
              <Show when={pageChanging()}>
                <div class="absolute inset-0 bg-bg-overlay/50 flex items-center justify-center">
                  <Spinner />
                </div>
              </Show>
            </div>
            <div class="flex justify-between items-center">
              <Pagination page={page()} total={total()} pageSize={pageSize} onChange={handlePageChange} />
            </div>
          </div>
        </Show>
      </Show>

      {/* 密码重置 Modal */}
      <Modal open={!!resetTarget()} onClose={handleCloseResetModal} title={resetTitle()} size="sm">
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
              onClick={() => {
                if (keyLoading()) return;
                setResetMode('key-result');
                void handleGenerateKey();
              }}
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
                  variant="ghost"
                  class="px-2"
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
                <Button onClick={handleCloseResetModal}>关闭</Button>
              </Show>
            </div>
          </div>
        </Show>
      </Modal>

      {/* 关闭 key-result 时密钥未复制 → 二次确认（密钥不可恢复） */}
      <ConfirmDialog
        open={showCloseKeyConfirm()}
        title="密钥关闭后将丢失"
        message="未复制密钥，关闭后将丢失（无法再次查看），确认关闭？"
        confirmText="确认关闭"
        variant="danger"
        onConfirm={() => { setShowCloseKeyConfirm(false); closeResetModal(); }}
        onCancel={() => setShowCloseKeyConfirm(false)}
      />

      {/* 行详情 Drawer */}
      <Drawer
        open={!!detailUser()}
        onClose={() => setDetailUser(null)}
        title="用户详情"
        width={380}
        headerActions={
          <Show when={detailUser()}>
            <Badge variant={detailUser()!.isBanned ? 'error' : 'success'} size="sm" dot>
              {detailUser()!.isBanned ? '已封禁' : '正常'}
            </Badge>
          </Show>
        }
        footer={
          <Show when={detailUser()}>
            {(u) => (
              <div class="flex flex-wrap justify-end gap-2">
                <Button variant="ghost" size="sm" onClick={() => setDetailUser(null)}>关闭</Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => { setDetailUser(null); closeResetModal(); setResetTarget(u()); }}
                >
                  重置密码
                </Button>
                <Button
                  size="sm"
                  variant={u().isBanned ? 'success' : 'danger'}
                  loading={busyUserId() === u().id}
                  onClick={() => handleBanClick(u())}
                >
                  {u().isBanned ? '解封' : '封禁'}
                </Button>
              </div>
            )}
          </Show>
        }
      >
        <Show when={detailUser()}>
          {(u) => (
            <div class="space-y-5 text-[13px]">
              {/* 元信息 */}
              <dl class="grid grid-cols-[80px_1fr] gap-y-2 gap-x-3 text-content-secondary">
                <dt>用户 ID</dt>
                <dd class="font-mono text-[11.5px] text-content break-all">{u().id}</dd>
                <dt>用户名</dt>
                <dd class="text-content">{u().username}</dd>
                <dt>邮箱</dt>
                <dd class="font-mono text-[11.5px] text-content">{maskEmail(u().email)}</dd>
                <dt>状态</dt>
                <dd>
                  <Badge variant={u().isBanned ? 'error' : 'success'} size="sm" dot>
                    {u().isBanned ? '已封禁' : '正常'}
                  </Badge>
                </dd>
              </dl>

              {/* m022:答题档案 (profile API) */}
              <section>
                <div class="flex items-baseline justify-between mb-2">
                  <h3 class="text-sm font-semibold text-content">答题档案</h3>
                  <Show when={profile()}>
                    <span class="text-[11.5px] text-content-tertiary tabular-nums">
                      {profile()!.sessionCount} 会话
                    </span>
                  </Show>
                </div>
                <Show
                  when={!profile.loading && profile()}
                  fallback={
                    <Show when={profile.error} fallback={<Spinner size="sm" />}>
                      <p class="text-[11.5px] text-error">{profile.error instanceof Error ? profile.error.message : '加载失败'}</p>
                    </Show>
                  }
                >
                  {(p) => (
                    <div class="space-y-2">
                      <div class="grid grid-cols-3 gap-2">
                        <div class="rounded-md bg-surface-secondary p-2 text-center">
                          <p class="text-[10.5px] text-content-tertiary uppercase tracking-wide">总答题</p>
                          <p class="text-base font-mono tabular-nums text-content">{p().totalRecords}</p>
                        </div>
                        <div class="rounded-md bg-surface-secondary p-2 text-center">
                          <p class="text-[10.5px] text-content-tertiary uppercase tracking-wide">正确</p>
                          <p class="text-base font-mono tabular-nums text-success">{p().correctRecords}</p>
                        </div>
                        <div class="rounded-md bg-surface-secondary p-2 text-center">
                          <p class="text-[10.5px] text-content-tertiary uppercase tracking-wide">正确率</p>
                          <p class="text-base font-mono tabular-nums text-content">
                            {p().accuracy != null ? `${((p().accuracy ?? 0) * 100).toFixed(1)}%` : '—'}
                          </p>
                        </div>
                      </div>
                      {/* 词书分布 top 10 mini bar */}
                      <Show when={p().wordbookDistribution.length > 0} fallback={
                        <p class="text-[11.5px] text-content-tertiary italic">尚无词书答题记录</p>
                      }>
                        <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary">词书分布 Top {Math.min(p().wordbookDistribution.length, 10)}</p>
                        <ul class="space-y-1">
                          <For each={p().wordbookDistribution.slice(0, 10)}>
                            {(wb) => {
                              const maxCount = Math.max(...p().wordbookDistribution.map((x) => x.recordCount));
                              const pct = maxCount > 0 ? (wb.recordCount / maxCount) * 100 : 0;
                              return (
                                <li>
                                  <div class="flex items-baseline justify-between text-[11.5px] gap-2">
                                    <span class="text-content truncate" title={wb.name}>{wb.name}</span>
                                    <span class="text-content-tertiary tabular-nums font-mono">{wb.recordCount}</span>
                                  </div>
                                  <div class="h-1 bg-surface-secondary rounded-full overflow-hidden">
                                    <div class="h-full bg-accent" style={{ width: `${pct}%` }} />
                                  </div>
                                </li>
                              );
                            }}
                          </For>
                        </ul>
                      </Show>
                    </div>
                  )}
                </Show>
              </section>

              {/* m022:设备会话(sessions API) */}
              <section>
                <div class="flex items-baseline justify-between mb-2">
                  <h3 class="text-sm font-semibold text-content">最近学习会话</h3>
                  <Show when={sessions()}>
                    <span class="text-[11.5px] text-content-tertiary tabular-nums">{sessions()!.length} 条</span>
                  </Show>
                </div>
                <Show
                  when={!sessionsResp.loading && sessions()}
                  fallback={
                    <Show when={sessionsResp.error} fallback={<Spinner size="sm" />}>
                      <p class="text-[11.5px] text-error">{sessionsResp.error instanceof Error ? sessionsResp.error.message : '加载失败'}</p>
                    </Show>
                  }
                >
                  {(rows) => (
                    <Show when={rows().length > 0} fallback={<p class="text-[11.5px] text-content-tertiary italic">尚无会话记录</p>}>
                      <ul class="space-y-1.5 max-h-64 overflow-y-auto pr-1">
                        <For each={rows()}>
                          {(s) => (
                            <li class="px-2.5 py-1.5 rounded-md bg-surface-secondary text-[11.5px]">
                              <div class="flex items-center justify-between gap-2">
                                <Badge variant={SESSION_STATUS_VARIANT[s.status] ?? 'default'} size="sm" dot>
                                  {SESSION_STATUS_LABEL[s.status] ?? s.status}
                                </Badge>
                                <time class="font-mono tabular-nums text-content-tertiary">
                                  {new Date(s.createdAt).toLocaleString('zh-CN', { hour12: false, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}
                                </time>
                              </div>
                              <div class="mt-1 flex items-center gap-3 text-content-tertiary tabular-nums">
                                <span>{s.wordsCount ?? 0} 词</span>
                                <span>正确 {s.correctCount ?? 0}</span>
                                <Show when={s.accuracy != null}>
                                  <span>{((s.accuracy ?? 0) * 100).toFixed(0)}%</span>
                                </Show>
                              </div>
                            </li>
                          )}
                        </For>
                      </ul>
                    </Show>
                  )}
                </Show>
              </section>
            </div>
          )}
        </Show>
      </Drawer>
    </div>
  );
}
