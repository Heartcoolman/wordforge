import { createSignal, createMemo, createEffect, createResource, Show, For, onMount, onCleanup } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Modal } from '@/components/ui/Modal';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Drawer } from '@/components/ui/Drawer';
import { Input } from '@/components/ui/Input';
import { Pagination } from '@/components/ui/Pagination';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { Tabs } from '@/components/ui/Tabs';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import type { AdminUser, AdminUsersQuery } from '@/types/admin';
import { cn } from '@/utils/cn';
import { formatDate, formatNumber, formatPercent, formatRelativeTime } from '@/utils/formatters';

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

const ROLE_LABEL: Record<string, string> = { user: '普通', staff: '员工', admin: '管理员' };
const STATUS_LABEL: Record<string, string> = { active: '活跃', inactive: '闲置', suspended: '已禁用' };

type ChipKey = 'all' | 'active' | 'inactive7d' | 'banned' | 'admin';
const FILTER_CHIPS: Array<{ key: ChipKey; label: string }> = [
  { key: 'all', label: '全部' },
  { key: 'active', label: '活跃' },
  { key: 'inactive7d', label: '7 天未登录' },
  { key: 'banned', label: '禁用' },
  { key: 'admin', label: '管理员' },
];

const PAGE_SIZE = 20;

// 头像底色:user.id hash 到 oklch hue,固定亮度饱和度,保证可读对比
function avatarColor(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return `oklch(70% 0.14 ${h % 360})`;
}

/** 对邮箱进行部分脱敏,如 test@example.com -> t***@example.com */
function maskEmail(email: string): string {
  const atIndex = email.indexOf('@');
  if (atIndex <= 1) return email;
  return email[0] + '***' + email.slice(atIndex);
}

// 重置模式选择卡片:复用 Card interactive variant 的标准 hover/elevation
function ResetModeCard(props: { title: string; description: string; onClick: () => void }) {
  return (
    <Card variant="interactive" padding="md" onClick={props.onClick}>
      <p class="font-medium text-content">{props.title}</p>
      <p class="text-caption mt-1">{props.description}</p>
    </Card>
  );
}

// 最近 20 题 mini grid:0=错,1=对,-1=占位
function MiniGrid(props: { outcomes: number[] }) {
  const cells = createMemo(() => {
    const arr = [...props.outcomes].slice(0, 20);
    while (arr.length < 20) arr.push(-1);
    return arr;
  });
  return (
    <div class="grid grid-cols-10 gap-[2px] w-[88px]" aria-label="最近 20 题答题结果">
      <For each={cells()}>
        {(v) => (
          <span
            class={cn(
              'aspect-square rounded-[1.5px]',
              v === 1 ? 'bg-success' : v === 0 ? 'bg-error' : 'bg-surface-tertiary',
            )}
          />
        )}
      </For>
    </div>
  );
}

type ResetMode = 'choose' | 'direct' | 'key-result';

export default function UserManagementPage() {
  const [users, setUsers] = createSignal<AdminUser[]>([]);
  const [total, setTotal] = createSignal(0);
  const [page, setPage] = createSignal(1);
  // 首次进入用 loading(整页 Spinner),翻页 / 切 filter 用 pageChanging(覆盖层不闪表格)
  const [loading, setLoading] = createSignal(true);
  const [pageChanging, setPageChanging] = createSignal(false);
  const [confirmTarget, setConfirmTarget] = createSignal<AdminUser | null>(null);
  // 行级 busy 标记:防止用户在某一行操作进行中重复点击
  const [busyUserId, setBusyUserId] = createSignal<string | null>(null);

  // 顶部 KPI 数据
  const [stats] = createResource(() => adminApi.getStats().catch(() => null));
  const [engagement] = createResource(() => adminApi.getEngagement().catch(() => null));

  // 搜索 + 筛选 chip
  const [filterChip, setFilterChip] = createSignal<ChipKey>('all');
  const [searchQuery, setSearchQuery] = createSignal('');
  const [debouncedQuery, setDebouncedQuery] = createSignal('');
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  function onSearchInput(v: string) {
    setSearchQuery(v);
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => setDebouncedQuery(v.trim()), 300);
  }
  onCleanup(() => { if (searchTimer) clearTimeout(searchTimer); });

  // 密码重置 Modal 状态
  const [resetTarget, setResetTarget] = createSignal<AdminUser | null>(null);
  const [resetMode, setResetMode] = createSignal<ResetMode>('choose');
  const [directPassword, setDirectPassword] = createSignal('');
  const [directConfirm, setDirectConfirm] = createSignal('');
  const [directError, setDirectError] = createSignal('');
  const [directLoading, setDirectLoading] = createSignal(false);
  const [generatedKey, setGeneratedKey] = createSignal('');
  const [keyLoading, setKeyLoading] = createSignal(false);
  const [resetTitle, setResetTitle] = createSignal('重置密码');
  createEffect(() => {
    const u = resetTarget()?.username;
    if (u) setResetTitle(`重置密码 - ${u}`);
  });
  const [showCloseKeyConfirm, setShowCloseKeyConfirm] = createSignal(false);

  // 多选 + 详情 Drawer + 批量操作
  const [selectedIds, setSelectedIds] = createSignal<Set<string>>(new Set());
  const [detailUser, setDetailUser] = createSignal<AdminUser | null>(null);
  const [bulkBusy, setBulkBusy] = createSignal<'ban' | 'unban' | 'role' | 'reset' | 'delete' | null>(null);
  const [bulkRoleOpen, setBulkRoleOpen] = createSignal(false);

  // m026:UX 收尾 state
  const [openMenuId, setOpenMenuId] = createSignal<string | null>(null);
  const [density, setDensity] = createSignal<'comfortable' | 'compact'>('comfortable');
  const [moreFiltersOpen, setMoreFiltersOpen] = createSignal(false);
  const [colSettingsOpen, setColSettingsOpen] = createSignal(false);
  // 默认 4 高频列全亮;popover 让 admin 关掉降噪(状态/注册/最近活跃为基础列固定)
  const [visibleCols, setVisibleCols] = createSignal({
    role: true, recordCount: true, accuracy: true, last20: true,
  });
  // 批量删除 + 设备封禁二次确认 state
  const [bulkDeleteOpen, setBulkDeleteOpen] = createSignal(false);
  const [bulkDeleteReason, setBulkDeleteReason] = createSignal('');
  const [deviceBanTarget, setDeviceBanTarget] = createSignal<{ deviceId: string; platform: string } | null>(null);
  // 点击页面任意位置关闭三点菜单(简化的 click-outside)
  const closeMenuOnGlobalClick = () => setOpenMenuId(null);

  // m024:添加用户对话框
  const [createOpen, setCreateOpen] = createSignal(false);
  const [createEmail, setCreateEmail] = createSignal('');
  const [createUsername, setCreateUsername] = createSignal('');
  const [createPassword, setCreatePassword] = createSignal('');
  const [createRole, setCreateRole] = createSignal<'user' | 'staff' | 'admin'>('user');
  const [createError, setCreateError] = createSignal('');
  const [createLoading, setCreateLoading] = createSignal(false);

  // m022:Drawer 打开时拉 profile + sessions
  const [profile] = createResource(detailUser, async (u) => (u ? adminApi.userProfile(u.id) : null));
  const [sessionsResp] = createResource(detailUser, async (u) => (u ? adminApi.userSessions(u.id, 20) : null));
  const sessions = () => sessionsResp()?.sessions ?? null;
  // m024:Drawer 4 tab 数据源 —— 设备会话 + 操作日志
  type DetailTab = 'profile' | 'learning' | 'devices' | 'audit';
  const [detailTab, setDetailTab] = createSignal<DetailTab>('profile');
  // 每次打开新用户重置到 profile tab
  createEffect(() => {
    if (detailUser()) setDetailTab('profile');
  });
  const [devicesResp] = createResource(detailUser, async (u) => (u ? adminApi.userDevices(u.id, 20) : null));
  const devices = () => devicesResp()?.devices ?? null;
  const [auditResp] = createResource(detailUser, async (u) => (u ? adminApi.userAuditLog(u.id, 50) : null));
  const auditEntries = () => auditResp()?.entries ?? null;
  // m025:用户档案深化 + 用户自有活动日志
  const [extrasResp] = createResource(detailUser, async (u) => (u ? adminApi.userExtras(u.id) : null));
  const [activityResp] = createResource(detailUser, async (u) => (u ? adminApi.userActivityLog(u.id, 50) : null));
  const activityEntries = () => activityResp()?.entries ?? null;

  const allSelected = createMemo(() => {
    const list = users();
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
    const list = users();
    setSelectedIds((prev) => {
      if (list.every((u) => prev.has(u.id))) return new Set<string>();
      const next = new Set(prev);
      list.forEach((u) => next.add(u.id));
      return next;
    });
  }

  // chip / search 切到任何变化都回到 page 1 重 load
  let firstLoad = true;
  createEffect(() => {
    debouncedQuery();
    filterChip();
    if (firstLoad) { firstLoad = false; return; }
    setPage(1);
    setSelectedIds(new Set<string>());
    void load(1, { paging: true });
  });

  async function load(p?: number, opts?: { paging?: boolean }) {
    const paging = opts?.paging ?? false;
    if (paging) setPageChanging(true); else setLoading(true);

    const chip = filterChip();
    const query: AdminUsersQuery = {
      page: p ?? page(),
      perPage: PAGE_SIZE,
      search: debouncedQuery() || undefined,
      includeStats: true,
    };
    if (chip === 'active') query.status = 'active';
    else if (chip === 'banned') query.banned = true;
    else if (chip === 'admin') query.role = 'admin';
    else if (chip === 'inactive7d') query.inactiveDays = 7;

    try {
      const res = await adminApi.getUsers(query);
      setUsers(res.data);
      setTotal(res.total);
    } catch (err: unknown) {
      uiStore.toast.error('加载失败', err instanceof Error ? err.message : '');
    } finally {
      if (paging) setPageChanging(false); else setLoading(false);
    }
  }

  onMount(() => { void load(); });
  // m026:点外面关闭三点菜单 —— 用延迟绑定避开打开时的同步 click 冒泡,
  // happy-dom 对 stopPropagation 不可靠,故 menu 打开时才挂 listener,
  // 关闭后立刻摘除。
  createEffect(() => {
    const id = openMenuId();
    if (id === null) return;
    const t = setTimeout(() => {
      document.addEventListener('click', closeMenuOnGlobalClick, { once: true });
    }, 0);
    onCleanup(() => {
      clearTimeout(t);
      document.removeEventListener('click', closeMenuOnGlobalClick);
    });
  });

  async function bulkToggleBan(action: 'ban' | 'unban') {
    const ids = Array.from(selectedIds());
    if (ids.length === 0) return;
    setBulkBusy(action);
    try {
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
      void load(page(), { paging: true });
    }
  }

  // m026:批量重置密码(密钥不回写,服务端为每条独立生成,前端只汇报成功/失败)
  async function bulkResetPasswords() {
    const ids = Array.from(selectedIds());
    if (ids.length === 0) return;
    setBulkBusy('reset');
    try {
      const res = await adminApi.usersBulkResetPassword(ids);
      if (res.failed === 0) {
        uiStore.toast.success(`批量重置成功 (${res.succeeded});请单查每条密钥转发用户`);
      } else {
        uiStore.toast.error(`部分失败:${res.succeeded} 成功 / ${res.failed} 失败`);
      }
    } catch (e) {
      uiStore.toast.error('批量重置失败', e instanceof Error ? e.message : '');
    } finally {
      setSelectedIds(new Set<string>());
      setBulkBusy(null);
    }
  }

  // m026:批量删除(危险,需 reason)
  async function bulkDeleteUsers() {
    const ids = Array.from(selectedIds());
    if (ids.length === 0) return;
    const reason = bulkDeleteReason().trim();
    if (!reason) {
      uiStore.toast.warning('请填写删除原因');
      return;
    }
    setBulkBusy('delete');
    try {
      const res = await adminApi.usersBulkDelete(ids, reason);
      if (res.failed === 0) {
        uiStore.toast.success(`批量删除成功 (${res.succeeded})`);
      } else {
        uiStore.toast.error(`部分失败:${res.succeeded} 成功 / ${res.failed} 失败`);
      }
    } catch (e) {
      uiStore.toast.error('批量删除失败', e instanceof Error ? e.message : '');
    } finally {
      setBulkDeleteOpen(false);
      setBulkDeleteReason('');
      setSelectedIds(new Set<string>());
      setBulkBusy(null);
      void load(1, { paging: true });
      setPage(1);
    }
  }

  // m026:设备封禁(从 Drawer 设备会话调起,二次确认后 fire)
  async function confirmDeviceBan() {
    const target = deviceBanTarget();
    const detail = detailUser();
    if (!target || !detail) return;
    try {
      await adminApi.banUserDevice(detail.id, target.deviceId);
      uiStore.toast.success(`已封设备 ${target.platform}`);
    } catch (e) {
      uiStore.toast.error('封设备失败', e instanceof Error ? e.message : '');
    } finally {
      setDeviceBanTarget(null);
    }
  }

  // m024:批量变更角色
  async function bulkChangeRole(role: 'user' | 'staff' | 'admin') {
    const ids = Array.from(selectedIds());
    if (ids.length === 0) return;
    setBulkBusy('role');
    setBulkRoleOpen(false);
    try {
      const res = await adminApi.usersBulkRole(ids, role);
      if (res.failed === 0) {
        uiStore.toast.success(`角色批量变更成功 (${res.succeeded} → ${ROLE_LABEL[role]})`);
      } else {
        uiStore.toast.error(`部分失败:${res.succeeded} 成功 / ${res.failed} 失败`);
      }
    } catch (e) {
      uiStore.toast.error('批量变更角色失败', e instanceof Error ? e.message : '');
    } finally {
      setSelectedIds(new Set<string>());
      setBulkBusy(null);
      void load(page(), { paging: true });
    }
  }

  function exportUsersCsv() {
    const rows: string[][] = [['id', 'username', 'email', 'role', 'status', 'createdAt', 'lastLoginAt']];
    const targetUsers = selectedIds().size > 0
      ? users().filter((u) => selectedIds().has(u.id))
      : users();
    targetUsers.forEach((u) => {
      rows.push([
        u.id, u.username, maskEmail(u.email), u.role,
        u.isBanned ? 'banned' : u.status,
        u.createdAt,
        u.lastLoginAt ?? '',
      ]);
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
      void load(page(), { paging: true });
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
    if (!directPassword()) { setDirectError('请输入新密码'); return; }
    if (directPassword() !== directConfirm()) { setDirectError('两次密码输入不一致'); return; }
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
    if (keyLoading()) return;
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
  function handleCloseResetModal() {
    if (resetMode() === 'key-result' && generatedKey()) {
      setShowCloseKeyConfirm(true);
      return;
    }
    closeResetModal();
  }

  // m024:添加用户
  function closeCreateModal() {
    setCreateOpen(false);
    setCreateEmail('');
    setCreateUsername('');
    setCreatePassword('');
    setCreateRole('user');
    setCreateError('');
    setCreateLoading(false);
  }
  async function handleCreateSubmit(e: Event) {
    e.preventDefault();
    setCreateError('');
    if (!createEmail() || !createUsername() || !createPassword()) {
      setCreateError('邮箱 / 用户名 / 密码均为必填');
      return;
    }
    setCreateLoading(true);
    try {
      const u = await adminApi.createUser({
        email: createEmail().trim(),
        username: createUsername().trim(),
        password: createPassword(),
        role: createRole(),
      });
      uiStore.toast.success(`已创建 ${u.username}`);
      closeCreateModal();
      void load(1, { paging: true });
      setPage(1);
    } catch (err: unknown) {
      setCreateError(err instanceof Error ? err.message : '创建失败');
    } finally {
      setCreateLoading(false);
    }
  }

  // KPI 行文案:用户聚合 · 今日活跃 · 较昨日趋势
  const headerDesc = createMemo(() => {
    const s = stats();
    const e = engagement();
    if (!s || !e) return null;
    return { total: s.users, active: e.activeToday, trend: s.trend?.users?.value ?? null };
  });

  return (
    <div class="space-y-6">
      {/* m024:page-header(对齐 admin后端/users.html) */}
      <header class="flex flex-wrap items-start justify-between gap-4">
        <div class="min-w-0">
          <h1 class="text-[clamp(26px,3vw,34px)] font-bold tracking-tight text-content">用户管理</h1>
          <p class="mt-2 text-sm leading-relaxed text-content-secondary max-w-2xl">
            <Show
              when={headerDesc()}
              fallback={<span class="text-content-tertiary">正在加载用户聚合…</span>}
            >
              {(d) => (
                <>
                  <span class="font-mono tabular-nums text-content">{formatNumber(d().total)}</span> 个用户 ·{' '}
                  <span class="font-mono tabular-nums text-content">{formatNumber(d().active)}</span> 今日活跃
                  <Show when={d().trend != null}>
                    {' '}·{' '}
                    <span
                      class={cn(
                        'font-mono tabular-nums',
                        (d().trend ?? 0) > 0 ? 'text-success' : (d().trend ?? 0) < 0 ? 'text-error' : 'text-content-tertiary',
                      )}
                    >
                      {(d().trend ?? 0) > 0 ? '+' : ''}{d().trend}%
                    </span>{' '}
                    较昨日
                  </Show>。支持批量禁用 / 重置密码 / 角色变更 / 导出 CSV。
                </>
              )}
            </Show>
          </p>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Button variant="secondary" onClick={exportUsersCsv}>导出 CSV</Button>
          <Button onClick={() => setCreateOpen(true)}>+ 添加用户</Button>
        </div>
      </header>

      {/* 筛选 + 搜索工具栏 */}
      <div class="flex flex-wrap items-center gap-2 px-3 py-2.5 rounded-lg bg-surface-elevated shadow-elevation-1">
        <div class="w-full max-w-xs">
          <Input
            placeholder="搜索 邮箱 / 昵称 / 用户 ID…"
            value={searchQuery()}
            onInput={(e) => onSearchInput(e.currentTarget.value)}
          />
        </div>
        <div class="flex flex-wrap gap-1.5" role="radiogroup" aria-label="用户筛选">
          <For each={FILTER_CHIPS}>
            {(chip) => (
              <button
                type="button"
                role="radio"
                aria-checked={filterChip() === chip.key}
                onClick={() => setFilterChip(chip.key)}
                class={cn(
                  'px-3 py-1.5 rounded-full text-xs transition-colors',
                  filterChip() === chip.key
                    ? 'bg-accent-light text-accent border border-accent/20 font-medium'
                    : 'bg-surface-secondary hover:bg-surface-tertiary text-content-secondary border border-transparent',
                )}
              >
                {chip.label}
              </button>
            )}
          </For>
        </div>
        <div class="flex-1 min-w-0" />
        {/* m026:工具栏右侧 —— 更多筛选 / 密度切换 / 列设置(批量栏隐起时显示) */}
        <Show when={selectedIds().size === 0}>
          <button
            type="button"
            class="px-2.5 py-1.5 rounded-md text-xs bg-surface-secondary hover:bg-surface-tertiary text-content-secondary"
            onClick={() => setMoreFiltersOpen(true)}
            aria-label="更多筛选"
          >
            更多筛选
          </button>
          <button
            type="button"
            class="p-1.5 rounded-md hover:bg-surface-secondary text-content-secondary"
            onClick={() => setDensity((d) => (d === 'comfortable' ? 'compact' : 'comfortable'))}
            aria-label={density() === 'comfortable' ? '切换为紧凑' : '切换为标准'}
            title={density() === 'comfortable' ? '紧凑密度' : '标准密度'}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <line x1="3" y1="6" x2="21" y2="6" /><line x1="3" y1="12" x2="21" y2="12" /><line x1="3" y1="18" x2="21" y2="18" />
            </svg>
          </button>
          <div class="relative" onClick={(e) => e.stopPropagation()}>
            <button
              type="button"
              class="p-1.5 rounded-md hover:bg-surface-secondary text-content-secondary"
              onClick={() => setColSettingsOpen((v) => !v)}
              aria-label="列设置"
              title="列设置"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="4" y1="21" x2="4" y2="14" /><line x1="4" y1="10" x2="4" y2="3" />
                <line x1="12" y1="21" x2="12" y2="12" /><line x1="12" y1="8" x2="12" y2="3" />
                <line x1="20" y1="21" x2="20" y2="16" /><line x1="20" y1="12" x2="20" y2="3" />
                <line x1="1" y1="14" x2="7" y2="14" /><line x1="9" y1="8" x2="15" y2="8" />
                <line x1="17" y1="16" x2="23" y2="16" />
              </svg>
            </button>
            <Show when={colSettingsOpen()}>
              <div class="absolute right-0 mt-1 z-20 rounded-md bg-surface-elevated shadow-elevation-3 ring-1 ring-border-hairline py-1 min-w-[160px] text-xs">
                <p class="px-3 py-1.5 text-content-tertiary border-b border-border-hairline">显示列</p>
                <For each={[
                  { key: 'role', label: '角色' },
                  { key: 'recordCount', label: '答题数' },
                  { key: 'accuracy', label: '正确率' },
                  { key: 'last20', label: '最近 20 题' },
                ] as const}>
                  {(c) => (
                    <label class="flex items-center gap-2 px-3 py-1.5 hover:bg-surface-secondary cursor-pointer">
                      <input
                        type="checkbox"
                        checked={visibleCols()[c.key]}
                        onChange={(e) => setVisibleCols({ ...visibleCols(), [c.key]: e.currentTarget.checked })}
                        class="size-3 accent-accent"
                      />
                      <span class="text-content">{c.label}</span>
                    </label>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </Show>
        <Show when={selectedIds().size > 0}>
          <span class="text-[12.5px] text-content">已选 <span class="font-mono tabular-nums">{selectedIds().size}</span></span>
          <Button size="xs" variant="danger" loading={bulkBusy() === 'ban'} onClick={() => bulkToggleBan('ban')}>批量封禁</Button>
          <Button size="xs" variant="success" loading={bulkBusy() === 'unban'} onClick={() => bulkToggleBan('unban')}>批量解封</Button>
          {/* m026:批量重置密码 */}
          <Button size="xs" variant="outline" loading={bulkBusy() === 'reset'} onClick={bulkResetPasswords}>重置密码</Button>
          <div class="relative" onClick={(e) => e.stopPropagation()}>
            <Button size="xs" variant="outline" loading={bulkBusy() === 'role'} onClick={() => setBulkRoleOpen((v) => !v)}>变更角色 ▾</Button>
            <Show when={bulkRoleOpen()}>
              <div class="absolute right-0 mt-1 z-20 rounded-md bg-surface-elevated shadow-elevation-3 ring-1 ring-border-hairline py-1 min-w-[120px]">
                <For each={['user', 'staff', 'admin'] as const}>
                  {(r) => (
                    <button
                      type="button"
                      class="block w-full px-3 py-1.5 text-left text-xs hover:bg-surface-secondary text-content"
                      onClick={() => { setBulkRoleOpen(false); void bulkChangeRole(r); }}
                    >
                      {ROLE_LABEL[r]}
                    </button>
                  )}
                </For>
              </div>
            </Show>
          </div>
          {/* m026:批量删除(危险,二次确认 + reason) */}
          <Button size="xs" variant="danger" loading={bulkBusy() === 'delete'} onClick={() => setBulkDeleteOpen(true)}>删除…</Button>
          <Button size="xs" variant="ghost" onClick={() => setSelectedIds(new Set<string>())}>清空</Button>
        </Show>
      </div>

      {/* 确认弹窗 */}
      <Show when={confirmTarget()}>
        {(target) => (
          <ConfirmDialog
            open={true}
            title={target().isBanned ? '确认解封' : '确认封禁'}
            message={
              <>
                确定要{target().isBanned ? '解封' : '封禁'}用户 <span class="font-medium text-content">{target().username}</span> 吗？
                {!target().isBanned && '封禁后该用户将无法登录,所有活跃会话将被撤销。'}
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
          <Empty
            title={debouncedQuery() || filterChip() !== 'all' ? '无匹配用户' : '暂无用户'}
            description={debouncedQuery() || filterChip() !== 'all' ? '试调整筛选或搜索条件' : '目前还没有注册用户'}
          />
        }>
          <div class="relative overflow-x-auto rounded-xl border border-border-hairline shadow-elevation-1 bg-surface">
            <table class="w-full text-sm">
              <thead>
                <tr class="bg-surface-secondary/60 backdrop-blur-sm border-b border-border-hairline text-caption uppercase tracking-wide text-content-secondary">
                  <th class="px-3 py-3 w-9 text-center font-medium">
                    <input
                      type="checkbox"
                      aria-label="全选 / 全不选"
                      checked={allSelected()}
                      onChange={toggleSelectAll}
                      class="size-3.5 cursor-pointer accent-accent"
                    />
                  </th>
                  <th class="px-4 py-3 text-left font-medium">用户</th>
                  <th class={cn('px-3 py-3 text-left font-medium', !visibleCols().role && 'hidden')}>角色</th>
                  <th class="px-3 py-3 text-left font-medium">状态</th>
                  <th class="px-3 py-3 text-left font-medium">注册</th>
                  <th class="px-3 py-3 text-left font-medium">最近活跃</th>
                  <th class={cn('px-3 py-3 text-right font-medium', !visibleCols().recordCount && 'hidden')}>答题数</th>
                  <th class={cn('px-3 py-3 text-right font-medium', !visibleCols().accuracy && 'hidden')}>正确率</th>
                  <th class={cn('px-3 py-3 text-left font-medium', !visibleCols().last20 && 'hidden')}>最近 20 题</th>
                  <th class="px-4 py-3 text-right font-medium">操作</th>
                </tr>
              </thead>
              <tbody>
                <For each={users()}>
                  {(user) => {
                    const accuracy = () => {
                      const s = user.stats;
                      if (!s || s.recordCount === 0) return null;
                      return s.correctCount / s.recordCount;
                    };
                    return (
                      <tr
                        class="border-b border-border-hairline last:border-b-0 hover:bg-accent-light/40 transition-colors duration-fast ease-out-expo cursor-pointer"
                        onClick={(e) => {
                          const tgt = e.target as HTMLElement;
                          if (tgt.closest('input,button,a')) return;
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
                        <td class="px-4 py-3">
                          <div class="flex items-center gap-2.5 min-w-0">
                            <div
                              class="size-9 rounded-full flex-none grid place-items-center text-white text-sm font-semibold shadow-elevation-1"
                              style={{ background: avatarColor(user.id) }}
                              aria-hidden="true"
                            >
                              {user.username.charAt(0).toUpperCase()}
                            </div>
                            <div class="min-w-0">
                              <div class="font-medium text-content truncate">{user.username}</div>
                              <div class="text-[11.5px] text-content-tertiary font-mono truncate">
                                {maskEmail(user.email)} · {user.id.slice(0, 8)}
                              </div>
                            </div>
                          </div>
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-3 py-3', !visibleCols().role && 'hidden')}>
                          <Badge
                            variant={user.role === 'admin' ? 'info' : user.role === 'staff' ? 'warning' : 'default'}
                            size="sm"
                          >
                            {ROLE_LABEL[user.role] ?? user.role}
                          </Badge>
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-3 py-3')}>
                          <Show
                            when={user.isBanned}
                            fallback={
                              <Badge
                                variant={user.status === 'active' ? 'success' : 'warning'}
                                size="sm"
                                dot
                              >
                                {STATUS_LABEL[user.status] ?? user.status}
                              </Badge>
                            }
                          >
                            <Badge variant="error" size="sm" dot>已封禁</Badge>
                          </Show>
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-3 py-3')}>
                          <time class="font-mono text-[12px] text-content-secondary tabular-nums">
                            {formatDate(user.createdAt)}
                          </time>
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-3 py-3')}>
                          <span class="text-[12px] text-content-secondary">
                            {user.lastLoginAt ? formatRelativeTime(user.lastLoginAt) : '从未'}
                          </span>
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-3 py-3', 'text-right font-mono tabular-nums text-[13px] text-content', !visibleCols().recordCount && 'hidden')}>
                          {user.stats ? formatNumber(user.stats.recordCount) : '—'}
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-3 py-3', 'text-right font-mono tabular-nums text-[13px] text-content', !visibleCols().accuracy && 'hidden')}>
                          {accuracy() != null ? formatPercent(accuracy()!, 1) : '—'}
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-3 py-3', !visibleCols().last20 && 'hidden')}>
                          <MiniGrid outcomes={user.stats?.last20Outcomes ?? []} />
                        </td>
                        <td class={cn(density() === 'compact' ? 'px-3 py-1.5' : 'px-4 py-3')} onClick={(e) => e.stopPropagation()}>
                          {/* m026:三点菜单收起 */}
                          <div class="relative flex items-center justify-end" onClick={(e) => e.stopPropagation()}>
                            <button
                              type="button"
                              class="p-1.5 rounded-md hover:bg-surface-secondary text-content-secondary disabled:opacity-50"
                              disabled={busyUserId() === user.id}
                              aria-label={`${user.username} 更多操作`}
                              onClick={(e) => { e.stopPropagation(); setOpenMenuId(openMenuId() === user.id ? null : user.id); }}
                            >
                              <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.5" /><circle cx="12" cy="12" r="1.5" /><circle cx="12" cy="19" r="1.5" /></svg>
                            </button>
                            <Show when={openMenuId() === user.id}>
                              <div class="absolute right-0 top-full mt-1 z-20 rounded-md bg-surface-elevated shadow-elevation-3 ring-1 ring-border-hairline py-1 min-w-[140px] text-xs">
                                <button
                                  type="button"
                                  class="block w-full px-3 py-1.5 text-left hover:bg-surface-secondary text-content"
                                  onClick={() => { setOpenMenuId(null); closeResetModal(); setResetTarget(user); }}
                                >
                                  重置密码
                                </button>
                                <button
                                  type="button"
                                  class={cn(
                                    'block w-full px-3 py-1.5 text-left hover:bg-surface-secondary',
                                    user.isBanned ? 'text-success-strong' : 'text-error-strong',
                                  )}
                                  onClick={() => { setOpenMenuId(null); handleBanClick(user); }}
                                >
                                  {user.isBanned ? '解封' : '封禁'}
                                </button>
                              </div>
                            </Show>
                          </div>
                        </td>
                      </tr>
                    );
                  }}
                </For>
              </tbody>
            </table>
            <Show when={pageChanging()}>
              <div class="absolute inset-0 bg-bg-overlay/50 flex items-center justify-center">
                <Spinner />
              </div>
            </Show>
          </div>
          <div class="flex justify-between items-center flex-wrap gap-2">
            {/* m026:分页文案 "显示 A-B 共 N 条" */}
            <span class="text-[11.5px] text-content-secondary">
              显示 <span class="font-mono tabular-nums">{total() === 0 ? 0 : (page() - 1) * PAGE_SIZE + 1}</span>
              {' – '}
              <span class="font-mono tabular-nums">{Math.min(page() * PAGE_SIZE, total())}</span>
              {' 共 '}
              <span class="font-mono tabular-nums font-medium text-content">{formatNumber(total())}</span> 条
            </span>
            <Pagination page={page()} total={total()} pageSize={PAGE_SIZE} onChange={handlePageChange} />
          </div>
        </Show>
      </Show>

      {/* m024:添加用户 Modal */}
      <Modal open={createOpen()} onClose={closeCreateModal} title="添加用户" size="sm">
        <form onSubmit={handleCreateSubmit} class="space-y-3 mt-2">
          <Input
            label="邮箱"
            type="email"
            placeholder="user@example.com"
            value={createEmail()}
            disabled={createLoading()}
            onInput={(e) => setCreateEmail(e.currentTarget.value)}
          />
          <Input
            label="用户名"
            placeholder="2-50 字符,字母数字下划线"
            value={createUsername()}
            disabled={createLoading()}
            onInput={(e) => setCreateUsername(e.currentTarget.value)}
          />
          <Input
            label="初始密码"
            type="password"
            placeholder="至少 8 字符,含大小写字母 + 数字"
            value={createPassword()}
            disabled={createLoading()}
            onInput={(e) => setCreatePassword(e.currentTarget.value)}
          />
          <div>
            <label class="block text-xs text-content-secondary mb-1">角色</label>
            <div class="flex gap-1.5">
              <For each={['user', 'staff', 'admin'] as const}>
                {(r) => (
                  <button
                    type="button"
                    onClick={() => setCreateRole(r)}
                    class={cn(
                      'px-3 py-1.5 rounded-md text-xs transition-colors',
                      createRole() === r
                        ? 'bg-accent-light text-accent border border-accent/30 font-medium'
                        : 'bg-surface-secondary hover:bg-surface-tertiary text-content-secondary border border-transparent',
                    )}
                  >
                    {ROLE_LABEL[r]}
                  </button>
                )}
              </For>
            </div>
          </div>
          <Show when={createError()}>
            <p class="text-sm text-error">{createError()}</p>
          </Show>
          <div class="flex justify-end gap-2 pt-2">
            <Button variant="ghost" onClick={closeCreateModal} disabled={createLoading()}>取消</Button>
            <Button type="submit" loading={createLoading()}>创建</Button>
          </div>
        </form>
      </Modal>

      {/* 密码重置 Modal */}
      <Modal open={!!resetTarget()} onClose={handleCloseResetModal} title={resetTitle()} size="sm">
        <Show when={resetMode() === 'choose'}>
          <div class="space-y-3 mt-2">
            <ResetModeCard
              title="直接重置密码"
              description="由管理员设定新密码,用户现有会话将被注销"
              onClick={() => setResetMode('direct')}
            />
            <ResetModeCard
              title="生成重置密钥"
              description="生成一次性密钥发送给用户,由用户自行修改密码"
              onClick={() => {
                if (keyLoading()) return;
                setResetMode('key-result');
                void handleGenerateKey();
              }}
            />
          </div>
        </Show>

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

        <Show when={resetMode() === 'key-result'}>
          <div class="space-y-4 mt-2">
            <Show when={keyLoading()}>
              <div class="flex justify-center py-6"><Spinner /></div>
            </Show>
            <Show when={generatedKey()}>
              <p class="text-sm text-content-secondary">请将以下密钥发送给用户:</p>
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
              <p class="text-xs text-content-tertiary">密钥有效期 24 小时,使用后自动失效</p>
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

      {/* 关闭 key-result 时密钥未复制 → 二次确认 */}
      <ConfirmDialog
        open={showCloseKeyConfirm()}
        title="密钥关闭后将丢失"
        message="未复制密钥,关闭后将丢失(无法再次查看),确认关闭?"
        confirmText="确认关闭"
        variant="danger"
        onConfirm={() => { setShowCloseKeyConfirm(false); closeResetModal(); }}
        onCancel={() => setShowCloseKeyConfirm(false)}
      />

      {/* m026:批量删除 Modal(reason 输入 + 二次确认) */}
      <Modal open={bulkDeleteOpen()} onClose={() => { setBulkDeleteOpen(false); setBulkDeleteReason(''); }} title={`删除 ${selectedIds().size} 个用户`} size="sm">
        <div class="space-y-3 mt-2">
          <p class="text-sm text-error">
            <strong>不可逆操作</strong>:将级联清理所有 USER_SCOPED 表(答题记录 / session / 词书 / 收藏 / 反馈等)。
          </p>
          <Input
            label="删除原因(必填,落入审计)"
            placeholder="例:测试账户清理 / GDPR 删除请求 / ..."
            value={bulkDeleteReason()}
            disabled={bulkBusy() === 'delete'}
            onInput={(e) => setBulkDeleteReason(e.currentTarget.value)}
          />
          <div class="flex justify-end gap-2 pt-2">
            <Button variant="ghost" onClick={() => { setBulkDeleteOpen(false); setBulkDeleteReason(''); }} disabled={bulkBusy() === 'delete'}>取消</Button>
            <Button variant="danger" loading={bulkBusy() === 'delete'} onClick={bulkDeleteUsers}>确认删除</Button>
          </div>
        </div>
      </Modal>

      {/* m026:设备封禁 ConfirmDialog */}
      <Show when={deviceBanTarget()}>
        {(t) => (
          <ConfirmDialog
            open={true}
            title="封禁设备"
            message={
              <>
                确认封禁设备 <span class="font-medium text-content">{t().platform}</span> 吗?
                该设备将无法再访问 <code class="font-mono text-xs">/api/*</code>,该用户的所有会话也会被一并撤销。
              </>
            }
            confirmText="确认封禁"
            variant="danger"
            onConfirm={confirmDeviceBan}
            onCancel={() => setDeviceBanTarget(null)}
          />
        )}
      </Show>

      {/* m026:更多筛选 Modal(当前 5 chip filter 已覆盖主流场景;此 Modal 留作高级条件入口) */}
      <Modal open={moreFiltersOpen()} onClose={() => setMoreFiltersOpen(false)} title="更多筛选" size="sm">
        <div class="space-y-3 mt-2 text-sm">
          <p class="text-content-secondary">
            当前已有 chip 过滤:全部 / 活跃 / 7 天未登录 / 禁用 / 管理员。
            自由组合 role × status × inactiveDays 的高级条件功能开发中,临时请用上方 chip 切换。
          </p>
          <div class="flex justify-end pt-2">
            <Button variant="ghost" onClick={() => setMoreFiltersOpen(false)}>关闭</Button>
          </div>
        </div>
      </Modal>

      {/* 行详情 Drawer(c8 将重做为 4 tab,本 commit 保留旧版) */}
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
            <div class="space-y-4 text-[13px]">
              {/* m024:4 tab 切换 */}
              <Tabs
                tabs={[
                  { id: 'profile', label: '资料' },
                  { id: 'learning', label: '答题档案' },
                  { id: 'devices', label: '设备会话' },
                  { id: 'audit', label: '操作日志' },
                ]}
                active={detailTab()}
                onChange={(id) => setDetailTab(id as DetailTab)}
              />

              {/* 资料 tab */}
              <Show when={detailTab() === 'profile'}>
                <dl class="grid grid-cols-[88px_1fr] gap-y-2 gap-x-3 text-content-secondary">
                  <dt>用户 ID</dt>
                  <dd class="font-mono text-[11.5px] text-content break-all">{u().id}</dd>
                  <dt>用户名</dt>
                  <dd class="text-content">{u().username}</dd>
                  <dt>邮箱</dt>
                  <dd class="font-mono text-[11.5px] text-content">{maskEmail(u().email)}</dd>
                  <dt>角色</dt>
                  <dd>
                    <Badge variant={u().role === 'admin' ? 'info' : u().role === 'staff' ? 'warning' : 'default'} size="sm">
                      {ROLE_LABEL[u().role] ?? u().role}
                    </Badge>
                  </dd>
                  <dt>状态</dt>
                  <dd>
                    <Show when={u().isBanned} fallback={
                      <Badge variant={u().status === 'active' ? 'success' : 'warning'} size="sm" dot>
                        {STATUS_LABEL[u().status] ?? u().status}
                      </Badge>
                    }>
                      <Badge variant="error" size="sm" dot>已封禁</Badge>
                    </Show>
                  </dd>
                  <dt>注册</dt>
                  <dd class="font-mono text-[12px] text-content">{formatDate(u().createdAt)}</dd>
                  <dt>注册来源</dt>
                  <dd class="text-content">{u().referrerSource ?? <span class="text-content-tertiary">—</span>}</dd>
                  <dt>最近活跃</dt>
                  <dd class="font-mono text-[12px] text-content">{u().lastLoginAt ? formatRelativeTime(u().lastLoginAt!) : '从未'}</dd>
                  {/* m025:extras 衍生字段 —— 等级 ELO / 每日目标 / 连击 / 语言时区 */}
                  <Show when={extrasResp()?.elo}>
                    {(elo) => (
                      <>
                        <dt>等级</dt>
                        <dd class="text-content">
                          等级 <span class="font-mono tabular-nums">{elo().level}</span>
                          {' · '}
                          ELO <span class="font-mono tabular-nums">{Math.round(elo().rating)}</span>
                          {' '}± <span class="font-mono tabular-nums">{Math.round(elo().sigma)}</span>
                          {' '}
                          <span class="text-content-tertiary text-[11px]">({elo().games} 局)</span>
                        </dd>
                      </>
                    )}
                  </Show>
                  <Show when={extrasResp()?.habit}>
                    {(habit) => (
                      <>
                        <dt>每日目标</dt>
                        <dd class="text-content font-mono tabular-nums">
                          {habit().dailyGoalWords} 词 / {habit().dailyGoalMinutes} 分钟
                        </dd>
                      </>
                    )}
                  </Show>
                  <Show when={extrasResp() && extrasResp()!.streakDays > 0}>
                    <dt>本周连击</dt>
                    <dd class="text-content font-mono tabular-nums">{extrasResp()!.streakDays} 天</dd>
                  </Show>
                  {/* m026:已选词书(JOIN study_configs × wordbooks) */}
                  <Show when={extrasResp() && extrasResp()!.selectedWordbooks.length > 0}>
                    <dt>词书</dt>
                    <dd class="text-content">
                      <For each={extrasResp()!.selectedWordbooks}>
                        {(wb, i) => (
                          <>
                            <Show when={i() > 0}><span class="text-content-tertiary"> · </span></Show>
                            <span>{wb.name}</span>
                          </>
                        )}
                      </For>
                    </dd>
                  </Show>
                  <Show when={extrasResp()?.preferences}>
                    {(pref) => (
                      <>
                        <dt>语言</dt>
                        <dd class="font-mono text-[12px] text-content">{pref().language}</dd>
                      </>
                    )}
                  </Show>
                  <Show when={extrasResp()?.latestDevice?.timezone}>
                    <dt>时区</dt>
                    <dd class="font-mono text-[12px] text-content">{extrasResp()!.latestDevice!.timezone}</dd>
                  </Show>
                </dl>
              </Show>

              {/* 答题档案 tab */}
              <Show when={detailTab() === 'learning'}>
                <Show
                  when={!profile.loading && profile()}
                  fallback={
                    <Show when={profile.error} fallback={<div class="flex justify-center py-4"><Spinner size="sm" /></div>}>
                      <p class="text-[11.5px] text-error">{profile.error instanceof Error ? profile.error.message : '加载失败'}</p>
                    </Show>
                  }
                >
                  {(p) => (
                    <div class="space-y-3">
                      {/* m026:4 列 —— 加 "平均反应" */}
                      <div class="grid grid-cols-4 gap-2">
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
                        <div class="rounded-md bg-surface-secondary p-2 text-center">
                          <p class="text-[10.5px] text-content-tertiary uppercase tracking-wide">平均反应</p>
                          <p class="text-base font-mono tabular-nums text-content">
                            {p().avgResponseTimeMs != null ? `${(p().avgResponseTimeMs! / 1000).toFixed(2)}s` : '—'}
                          </p>
                        </div>
                      </div>
                      {/* m025:7d 窗口 + 疲劳触发(extras) */}
                      <Show when={extrasResp()?.metrics7d}>
                        {(m) => (
                          <div class="grid grid-cols-2 gap-2">
                            <div class="rounded-md bg-surface-secondary p-2 text-center">
                              <p class="text-[10.5px] text-content-tertiary uppercase tracking-wide">7d 正确率</p>
                              <p class="text-base font-mono tabular-nums text-content">
                                {m().accuracy != null ? `${((m().accuracy ?? 0) * 100).toFixed(1)}%` : '—'}
                                <span class="text-[10.5px] text-content-tertiary ml-1">({m().records})</span>
                              </p>
                            </div>
                            <div class={cn('rounded-md p-2 text-center', m().fatigueAlertCount > 0 ? 'bg-warning-light/30' : 'bg-surface-secondary')}>
                              <p class="text-[10.5px] text-content-tertiary uppercase tracking-wide">7d 疲劳触发</p>
                              <p class={cn('text-base font-mono tabular-nums', m().fatigueAlertCount > 0 ? 'text-warning-strong' : 'text-content')}>
                                {m().fatigueAlertCount}
                              </p>
                            </div>
                          </div>
                        )}
                      </Show>
                      {/* m024:最近 20 题 mini grid — 直接复用 list 上 user.stats.last20Outcomes */}
                      <Show when={u().stats && u().stats!.last20Outcomes.length > 0}>
                        <div>
                          <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1.5">
                            最近 20 题 · 绿=正确 / 红=错误
                          </p>
                          <MiniGrid outcomes={u().stats!.last20Outcomes} />
                        </div>
                      </Show>
                      {/* m025:AMAS 当前策略 mono 行 */}
                      <Show when={extrasResp()?.latestStrategy}>
                        {(s) => (
                          <div>
                            <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1">AMAS 当前策略</p>
                            <code class="block font-mono text-[10.5px] text-accent break-all bg-surface-secondary px-2 py-1 rounded">
                              {(() => {
                                const st = s().strategy as Record<string, unknown>;
                                const parts: string[] = [];
                                if (typeof st.algorithm_id === 'string') parts.push(String(st.algorithm_id));
                                if (typeof st.batch_size === 'number') parts.push(`batch=${st.batch_size}`);
                                if (typeof st.new_ratio === 'number') parts.push(`new=${(st.new_ratio as number).toFixed(2)}`);
                                if (typeof st.difficulty === 'number') parts.push(`diff=${(st.difficulty as number).toFixed(2)}`);
                                return parts.length > 0 ? parts.join(' · ') : JSON.stringify(st).slice(0, 80);
                              })()}
                            </code>
                          </div>
                        )}
                      </Show>
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
                      <p class="text-[11.5px] text-content-tertiary">
                        最近学习会话 <span class="font-mono">{sessions()?.length ?? 0}</span> 条
                      </p>
                    </div>
                  )}
                </Show>
              </Show>

              {/* 设备会话 tab */}
              <Show when={detailTab() === 'devices'}>
                <div class="space-y-3">
                  {/* m025:最新 telemetry 画像(OS / 浏览器 / 时区) */}
                  <Show when={extrasResp()?.latestDevice}>
                    {(d) => (
                      <div class="rounded-md bg-info-light/30 px-3 py-2 text-[11.5px]">
                        <p class="text-[10.5px] uppercase tracking-wide text-content-tertiary mb-1">最近 telemetry 画像</p>
                        <div class="flex flex-wrap gap-x-3 gap-y-1 font-mono tabular-nums text-content">
                          <Show when={d().osName}><span>{d().osName}</span></Show>
                          <Show when={d().browserName}>
                            <span>· {d().browserName}{d().browserVersion ? ` ${d().browserVersion}` : ''}</span>
                          </Show>
                          <Show when={d().timezone}><span>· {d().timezone}</span></Show>
                          <Show when={d().language}><span>· {d().language}</span></Show>
                        </div>
                      </div>
                    )}
                  </Show>
                  <Show
                    when={!devicesResp.loading && devices()}
                    fallback={
                      <Show when={devicesResp.error} fallback={<div class="flex justify-center py-4"><Spinner size="sm" /></div>}>
                        <p class="text-[11.5px] text-error">{devicesResp.error instanceof Error ? devicesResp.error.message : '加载失败'}</p>
                      </Show>
                    }
                  >
                    {(rows) => (
                      <Show when={rows().length > 0} fallback={<p class="text-[11.5px] text-content-tertiary italic">该用户尚无关联设备</p>}>
                        <ul class="space-y-1.5">
                          <For each={rows()}>
                            {(d) => (
                              <li class="flex items-center justify-between gap-2 px-2.5 py-2 rounded-md bg-surface-secondary text-[11.5px]">
                                <div class="min-w-0">
                                  <div class="flex items-center gap-2 mb-0.5">
                                    <strong class="text-content">{d.platform}</strong>
                                    <Show when={d.appVersion}>
                                      <span class="font-mono text-[10.5px] text-content-tertiary">v{d.appVersion}</span>
                                    </Show>
                                    <Show when={d.isBanned}>
                                      <Badge variant="error" size="sm">已禁</Badge>
                                    </Show>
                                  </div>
                                  <div class="font-mono text-[10.5px] text-content-tertiary truncate">
                                    {d.deviceId.slice(0, 16)} · {formatRelativeTime(d.lastSeenAt)}
                                  </div>
                                </div>
                                {/* m026:设备封禁按钮(已封时隐) */}
                                <Show when={!d.isBanned}>
                                  <button
                                    type="button"
                                    class="shrink-0 px-2 py-1 text-[11px] rounded-md text-error-strong hover:bg-error-light/40"
                                    onClick={() => setDeviceBanTarget({ deviceId: d.deviceId, platform: d.platform })}
                                  >
                                    封设备
                                  </button>
                                </Show>
                              </li>
                            )}
                          </For>
                        </ul>
                      </Show>
                    )}
                  </Show>
                </div>
              </Show>

              {/* 操作日志 tab —— 两栏:admin 对该用户的操作 + 用户自有活动 */}
              <Show when={detailTab() === 'audit'}>
                <div class="space-y-4">
                  {/* 用户自有活动(m025 activity_log) */}
                  <section>
                    <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1.5">用户活动</p>
                    <Show
                      when={!activityResp.loading && activityEntries()}
                      fallback={
                        <Show when={activityResp.error} fallback={<div class="flex justify-center py-4"><Spinner size="sm" /></div>}>
                          <p class="text-[11.5px] text-error">{activityResp.error instanceof Error ? activityResp.error.message : '加载失败'}</p>
                        </Show>
                      }
                    >
                      {(rows) => (
                        <Show when={rows().length > 0} fallback={<p class="text-[11.5px] text-content-tertiary italic">该用户尚无活动记录</p>}>
                          <ul class="space-y-1 max-h-56 overflow-y-auto pr-1">
                            <For each={rows()}>
                              {(a) => (
                                <li class="flex items-start gap-2 px-2.5 py-1.5 rounded-md bg-surface-secondary text-[11.5px]">
                                  <time class="font-mono text-[10.5px] text-content-tertiary tabular-nums w-[88px] shrink-0 pt-0.5">
                                    {new Date(a.createdAt).toLocaleString('zh-CN', { hour12: false, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}
                                  </time>
                                  <div class="min-w-0 flex-1">
                                    <div class="flex items-center gap-2 flex-wrap">
                                      <code class="font-mono text-[11px] text-info-strong">{a.action}</code>
                                      <Show when={a.ip}>
                                        <span class="font-mono text-[10.5px] text-content-tertiary">{a.ip}</span>
                                      </Show>
                                    </div>
                                    <Show when={a.detailJson}>
                                      <code class="block mt-0.5 font-mono text-[10.5px] text-content-tertiary break-all">{a.detailJson}</code>
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

                  {/* admin 对该用户的操作(audit_log) */}
                  <section>
                    <p class="text-[11.5px] uppercase tracking-wide text-content-tertiary mb-1.5">admin 操作</p>
                    <Show
                      when={!auditResp.loading && auditEntries()}
                      fallback={
                        <Show when={auditResp.error} fallback={<div class="flex justify-center py-4"><Spinner size="sm" /></div>}>
                          <p class="text-[11.5px] text-error">{auditResp.error instanceof Error ? auditResp.error.message : '加载失败'}</p>
                        </Show>
                      }
                    >
                      {(rows) => (
                        <Show when={rows().length > 0} fallback={<p class="text-[11.5px] text-content-tertiary italic">尚无 admin 操作记录</p>}>
                          <ul class="space-y-1 max-h-56 overflow-y-auto pr-1">
                            <For each={rows()}>
                              {(a) => (
                                <li class="flex items-start gap-2 px-2.5 py-1.5 rounded-md bg-surface-secondary text-[11.5px]">
                                  <time class="font-mono text-[10.5px] text-content-tertiary tabular-nums w-[88px] shrink-0 pt-0.5">
                                    {new Date(a.startedAt).toLocaleString('zh-CN', { hour12: false, month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })}
                                  </time>
                                  <div class="min-w-0 flex-1">
                                    <div class="flex items-center gap-2">
                                      <code class="font-mono text-[11px] text-accent">{a.action}</code>
                                      <Show when={a.outcome !== 'success'}>
                                        <Badge variant="warning" size="sm">{a.outcome}</Badge>
                                      </Show>
                                    </div>
                                    <Show when={a.metadataJson}>
                                      <code class="block mt-0.5 font-mono text-[10.5px] text-content-tertiary break-all">{a.metadataJson}</code>
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
              </Show>
            </div>
          )}
        </Show>
      </Drawer>
    </div>
  );
}
