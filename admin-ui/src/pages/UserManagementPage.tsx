/* ============================================================
   用户管理 — list · filters · facets · create · ban · role · bulk · detail drawer
   handoff port (pages-users.jsx) → SolidJS + 真实后端 API
   ============================================================ */
import { createSignal, createResource, createMemo, createEffect, Show, For, type JSX } from 'solid-js';
import {
  Btn, Badge, Card, Field, Tabs, Empty, Skel, Loading,
  Modal, Drawer, Confirm, Pagination, PageHead, Icon,
  fmtNum, fmtTime, fmtAgo, sx, toast,
} from '@/components/wf';
import { adminApi } from '@/api/admin';
import type {
  AdminUser, AdminUsersQuery, AdminUsersPage, UserExtras,
  UserActivityEntry, AdminAuditEntry, ClientDeviceRow,
} from '@/types/admin';
import type { UserProfile, UserSessionRow } from '@/api/admin';

type Role = 'user' | 'staff' | 'admin';

const ROLE_LABEL: Record<string, string> = { user: '普通用户', staff: '员工', admin: '管理员' };
const ROLE_VARIANT: Record<string, 'default' | 'info' | 'accent'> = { user: 'default', staff: 'info', admin: 'accent' };

const PAGE_SIZE = 12;

type BulkKind = 'ban' | 'unban' | 'reset' | 'delete';
const BULK_LABEL: Record<BulkKind, string> = { ban: '封禁', unban: '解封', reset: '重置密码', delete: '删除' };

// list 查询窗口的依赖键
interface ListDep {
  search: string;
  role: Role | '';
  banned: boolean | null;
  status: 'active' | 'inactive' | 'suspended' | null;
  inactiveDays: number | null;
  page: number;
}

export default function UserManagementPage() {
  const [q, setQ] = createSignal('');
  const [debouncedQ, setDebouncedQ] = createSignal('');
  const [role, setRole] = createSignal<Role | ''>('');
  const [banned, setBanned] = createSignal<boolean | null>(null);
  const [status, setStatus] = createSignal<'active' | 'inactive' | 'suspended' | null>(null);
  const [inactiveDays, setInactiveDays] = createSignal<number | null>(null);
  const [page, setPage] = createSignal(1);
  const [sel, setSel] = createSignal<Set<string>>(new Set<string>());

  const [createOpen, setCreateOpen] = createSignal(false);
  const [detail, setDetail] = createSignal<AdminUser | null>(null);
  const [resetKey, setResetKey] = createSignal<{ user: AdminUser; key: string; hours: number } | null>(null);
  const [confirmBulk, setConfirmBulk] = createSignal<BulkKind | null>(null);
  const [busy, setBusy] = createSignal(false);

  // 搜索防抖
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  function onSearch(v: string) {
    setQ(v);
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => { setDebouncedQ(v.trim()); setPage(1); }, 300);
  }

  // facet chip 计数
  const [facets, { refetch: reloadFacets }] = createResource(() => adminApi.userFacets().catch(() => null));

  // 列表(窗口化资源:依赖键变化即重拉)
  const listDep = createMemo<ListDep>(() => ({
    search: debouncedQ(), role: role(), banned: banned(),
    status: status(), inactiveDays: inactiveDays(), page: page(),
  }));
  const [list, { refetch: reloadList }] = createResource<AdminUsersPage, ListDep>(listDep, async (dep) => {
    const query: AdminUsersQuery = {
      page: dep.page, perPage: PAGE_SIZE, includeStats: true,
      search: dep.search || undefined,
    };
    if (dep.role) query.role = dep.role;
    if (dep.banned != null) query.banned = dep.banned;
    if (dep.status) query.status = dep.status;
    if (dep.inactiveDays != null) query.inactiveDays = dep.inactiveDays;
    return adminApi.getUsers(query);
  });

  const rows = (): AdminUser[] => list()?.data ?? [];
  const allSel = createMemo(() => { const r = rows(); return r.length > 0 && r.every((u) => sel().has(u.id)); });

  function toggle(id: string) {
    setSel((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  }
  function toggleAll() {
    setSel(() => allSel() ? new Set<string>() : new Set(rows().map((r) => r.id)));
  }

  function reloadAll() { void reloadList(); void reloadFacets(); }

  async function doBan(u: AdminUser) {
    try {
      if (u.isBanned) { await adminApi.unbanUser(u.id); toast.success('已解封 ' + u.username); }
      else { await adminApi.banUser(u.id); toast.success('已封禁 ' + u.username); }
      reloadAll();
    } catch (e) {
      toast.error((u.isBanned ? '解封' : '封禁') + '失败', e instanceof Error ? e.message : '');
    }
  }

  async function doRole(u: AdminUser, r: Role) {
    try {
      await adminApi.patchUserRole(u.id, r);
      toast.success(u.username + ' 角色改为 ' + ROLE_LABEL[r]);
      setDetail((d) => (d && d.id === u.id ? { ...d, role: r } : d));
      reloadAll();
    } catch (e) {
      toast.error('变更角色失败', e instanceof Error ? e.message : '');
    }
  }

  async function doReset(u: AdminUser) {
    try {
      const r = await adminApi.resetUserPassword(u.id);
      setResetKey({ user: u, key: r.resetKey, hours: r.expiresInHours });
    } catch (e) {
      toast.error('生成重置密钥失败', e instanceof Error ? e.message : '');
    }
  }

  async function runBulk() {
    const kind = confirmBulk();
    if (!kind) return;
    setBusy(true);
    const ids = [...sel()];
    try {
      if (kind === 'ban') { const r = await adminApi.usersBulkBan(ids); bulkToast('已批量封禁', r); }
      else if (kind === 'unban') { const r = await adminApi.usersBulkUnban(ids); bulkToast('已批量解封', r); }
      else if (kind === 'reset') { const r = await adminApi.usersBulkResetPassword(ids); bulkToast('已批量重置', r); }
      else if (kind === 'delete') { const r = await adminApi.usersBulkDelete(ids, '批量清理'); bulkToast('已删除', r); }
      setSel(new Set<string>());
      setConfirmBulk(null);
      reloadAll();
    } catch (e) {
      toast.error('批量操作失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  }
  function bulkToast(prefix: string, r: { succeeded: number; failed: number }) {
    if (r.failed === 0) toast.success(`${prefix} ${r.succeeded} 个用户`);
    else toast.warning(`部分失败:${r.succeeded} 成功 / ${r.failed} 失败`);
  }

  // facet chips
  type ChipVal = { role: Role | ''; banned: boolean | null; status: 'active' | null; inactiveDays: number | null };
  const FACET_CHIPS = createMemo<Array<{ label: string; val: ChipVal; count: number | null; active: boolean }>>(() => {
    const f = facets();
    return [
      { label: '全部', val: { role: '', banned: null, status: null, inactiveDays: null }, count: f?.total ?? null, active: !role() && banned() == null && status() == null && inactiveDays() == null },
      { label: '活跃', val: { role: '', banned: null, status: 'active', inactiveDays: null }, count: f?.active ?? null, active: status() === 'active' && !role() },
      { label: '7 天未登录', val: { role: '', banned: null, status: null, inactiveDays: 7 }, count: f?.inactive7d ?? null, active: inactiveDays() === 7 },
      { label: '已封禁', val: { role: '', banned: true, status: null, inactiveDays: null }, count: f?.banned ?? null, active: banned() === true },
      { label: '管理员', val: { role: 'admin', banned: null, status: null, inactiveDays: null }, count: f?.admins ?? null, active: role() === 'admin' },
    ];
  });
  function applyChip(v: ChipVal) {
    setRole(v.role); setBanned(v.banned); setStatus(v.status); setInactiveDays(v.inactiveDays);
    setPage(1); setSel(new Set<string>());
  }

  const accuracyOf = (u: AdminUser): number | null => {
    const s = u.stats;
    if (!s || s.recordCount === 0) return null;
    return s.correctCount / s.recordCount;
  };

  return (
    <div>
      <PageHead
        title="用户管理"
        desc="账号检索、封禁、角色与密码管理,支持批量操作;所有变更写入审计日志。"
        right={<Btn variant="primary" icon="plus" onClick={() => setCreateOpen(true)}>添加用户</Btn>}
      />

      <Card pad={false} style={sx({ overflow: 'hidden' })}>
        {/* toolbar */}
        <div style={sx({ padding: 14, display: 'flex', gap: 10, flexWrap: 'wrap', alignItems: 'center', borderBottom: '1px solid var(--hairline)' })}>
          <div style={sx({ position: 'relative', flex: '1 1 240px', maxWidth: 340 })}>
            <span style={sx({ position: 'absolute', left: 11, top: 9, color: 'var(--text-3)' })}><Icon name="search" size={15} /></span>
            <input
              class="input"
              style={sx({ paddingLeft: 34 })}
              placeholder="搜索邮箱 / 用户名…"
              value={q()}
              onInput={(e) => onSearch(e.currentTarget.value)}
            />
          </div>
          <div style={sx({ display: 'flex', gap: 7, flexWrap: 'wrap' })}>
            <For each={FACET_CHIPS()}>
              {(c) => (
                <button
                  class="badge"
                  onClick={() => applyChip(c.val)}
                  style={sx({
                    cursor: 'pointer', padding: '5px 11px',
                    border: '1px solid ' + (c.active ? 'var(--accent)' : 'var(--border)'),
                    background: c.active ? 'var(--accent-soft)' : 'var(--surface-sunken)',
                    color: c.active ? 'var(--accent)' : 'var(--text-2)',
                  })}
                >
                  {c.label}
                  <Show when={c.count != null}>
                    <span class="mono" style={sx({ opacity: 0.7, marginLeft: 6 })}>{c.count}</span>
                  </Show>
                </button>
              )}
            </For>
          </div>
          <select
            class="select"
            style={sx({ width: 'auto', marginLeft: 'auto' })}
            value={role()}
            onChange={(e) => { setRole(e.currentTarget.value as Role | ''); setPage(1); setSel(new Set<string>()); }}
          >
            <option value="">全部角色</option>
            <option value="user">普通用户</option>
            <option value="staff">员工</option>
            <option value="admin">管理员</option>
          </select>
        </div>

        {/* bulk bar */}
        <Show when={sel().size > 0}>
          <div class="fade-in" style={sx({ padding: '10px 14px', display: 'flex', alignItems: 'center', gap: 10, background: 'var(--accent-soft)', borderBottom: '1px solid var(--hairline)' })}>
            <b style={sx({ fontSize: 13, color: 'var(--accent)' })}>已选 {sel().size} 个</b>
            <div style={sx({ display: 'flex', gap: 7, marginLeft: 'auto', flexWrap: 'wrap' })}>
              <Btn size="sm" variant="secondary" icon="ban" onClick={() => setConfirmBulk('ban')}>封禁</Btn>
              <Btn size="sm" variant="secondary" icon="check" onClick={() => setConfirmBulk('unban')}>解封</Btn>
              <Btn size="sm" variant="secondary" icon="key" onClick={() => setConfirmBulk('reset')}>重置密码</Btn>
              <Btn size="sm" variant="danger" icon="trash" onClick={() => setConfirmBulk('delete')}>删除</Btn>
              <Btn size="sm" variant="ghost" onClick={() => setSel(new Set<string>())}>取消</Btn>
            </div>
          </div>
        </Show>

        <div style={sx({ overflowX: 'auto' })}>
          <table class="tbl">
            <thead><tr>
              <th style={sx({ width: 36 })}><input type="checkbox" checked={allSel()} onChange={toggleAll} /></th>
              <th>用户</th><th>角色</th><th>状态</th><th>答题数</th><th>正确率</th><th>最后登录</th><th>注册时间</th>
              <th style={sx({ textAlign: 'right' })}>操作</th>
            </tr></thead>
            <tbody>
              <Show
                when={list.state !== 'pending'}
                fallback={<For each={Array.from({ length: 8 })}>{() => <tr><td colSpan={9}><Skel h={22} /></td></tr>}</For>}
              >
                <Show
                  when={rows().length > 0}
                  fallback={<tr><td colSpan={9}><Empty title={debouncedQ() || role() || banned() != null || status() || inactiveDays() != null ? '无匹配用户' : '暂无用户'} desc="试调整筛选或搜索条件" icon="users" /></td></tr>}
                >
                  <For each={rows()}>
                    {(u) => {
                      const acc = accuracyOf(u);
                      return (
                        <tr>
                          <td><input type="checkbox" checked={sel().has(u.id)} onChange={() => toggle(u.id)} /></td>
                          <td>
                            <div style={sx({ display: 'flex', alignItems: 'center', gap: 9 })}>
                              <span style={sx({ width: 30, height: 30, borderRadius: 50, flex: 'none', background: 'var(--grad-brand-soft)', color: 'var(--accent)', display: 'grid', placeItems: 'center', fontWeight: 700, fontSize: 12 })}>
                                {u.username.slice(0, 2).toUpperCase()}
                              </span>
                              <span style={sx({ minWidth: 0 })}>
                                <span style={sx({ display: 'block', fontWeight: 600, fontSize: 13 })}>{u.username}</span>
                                <span class="muted-3 mono" style={sx({ fontSize: 11 })}>{u.email}</span>
                              </span>
                            </div>
                          </td>
                          <td><Badge variant={ROLE_VARIANT[u.role]}>{ROLE_LABEL[u.role] ?? u.role}</Badge></td>
                          <td><Badge variant={u.isBanned ? 'error' : 'success'} dot>{u.isBanned ? '已封禁' : '正常'}</Badge></td>
                          <td class="mono tnum">{u.stats ? fmtNum(u.stats.recordCount) : '—'}</td>
                          <td class="mono tnum">{acc != null ? (acc * 100).toFixed(0) + '%' : '—'}</td>
                          <td class="muted" style={sx({ fontSize: 12 })}>{u.lastLoginAt ? fmtAgo(u.lastLoginAt) : '从未'}</td>
                          <td class="muted mono" style={sx({ fontSize: 11.5 })}>{fmtTime(u.createdAt).slice(0, 10)}</td>
                          <td>
                            <div style={sx({ display: 'flex', gap: 5, justifyContent: 'flex-end' })}>
                              <Btn size="xs" variant="ghost" onClick={() => setDetail(u)}>详情</Btn>
                              <Btn size="xs" variant={u.isBanned ? 'success' : 'danger'} onClick={() => void doBan(u)}>{u.isBanned ? '解封' : '封禁'}</Btn>
                            </div>
                          </td>
                        </tr>
                      );
                    }}
                  </For>
                </Show>
              </Show>
            </tbody>
          </table>
        </div>
        <div style={sx({ padding: 12 })}>
          <Pagination page={page()} totalPages={list()?.totalPages ?? 1} onPage={(p) => { setPage(p); setSel(new Set<string>()); }} />
        </div>
      </Card>

      {/* create */}
      <CreateUserModal open={createOpen()} onClose={() => setCreateOpen(false)} onDone={reloadAll} />

      {/* detail drawer */}
      <UserDrawer user={detail()} onClose={() => setDetail(null)} onBan={doBan} onRole={doRole} onReset={doReset} />

      {/* reset key modal */}
      <Modal
        open={!!resetKey()}
        onClose={() => setResetKey(null)}
        title="密码重置密钥"
        size="sm"
        footer={
          <Btn
            variant="primary"
            onClick={() => {
              const rk = resetKey();
              if (!rk) return;
              void navigator.clipboard?.writeText(rk.key);
              toast.success('已复制');
            }}
          >复制密钥</Btn>
        }
      >
        <Show when={resetKey()}>
          {(rk) => (
            <>
              <p class="muted" style={sx({ fontSize: 13, marginTop: 0 })}>
                已为 <b>{rk().user.username}</b> 生成重置密钥,{rk().hours} 小时内有效。
                <b style={sx({ color: 'var(--warning)' })}>关闭后将无法再次查看。</b>
              </p>
              <div class="mono" style={sx({ padding: 12, borderRadius: 10, background: 'var(--surface-sunken)', border: '1px solid var(--border)', fontSize: 12.5, wordBreak: 'break-all' })}>
                {rk().key}
              </div>
            </>
          )}
        </Show>
      </Modal>

      <Confirm
        open={!!confirmBulk()}
        onClose={() => setConfirmBulk(null)}
        onConfirm={() => void runBulk()}
        loading={busy()}
        danger={confirmBulk() === 'delete'}
        title={'批量' + (confirmBulk() ? BULK_LABEL[confirmBulk()!] : '') + ' ' + sel().size + ' 个用户'}
        body={confirmBulk() === 'delete' ? '删除不可恢复,且会清除用户全部数据。此操作将记录强审计。' : '该操作将应用到所有已选用户,并写入审计日志。'}
      />
    </div>
  );
}

/* ---------- 添加用户 ---------- */
function CreateUserModal(props: { open: boolean; onClose: () => void; onDone: () => void }) {
  const [email, setEmail] = createSignal('');
  const [username, setUsername] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [r, setR] = createSignal<Role>('user');
  const [busy, setBusy] = createSignal(false);

  createEffect(() => {
    if (props.open) { setEmail(''); setUsername(''); setPassword(''); setR('user'); setBusy(false); }
  });

  async function submit() {
    if (!email() || !username() || !password()) { toast.warning('请填写完整'); return; }
    setBusy(true);
    try {
      await adminApi.createUser({ email: email().trim(), username: username().trim(), password: password(), role: r() });
      toast.success('已创建用户 ' + username());
      props.onClose();
      props.onDone();
    } catch (e) {
      toast.error('创建失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={props.open}
      onClose={props.onClose}
      title="添加用户"
      size="sm"
      footer={
        <>
          <Btn variant="ghost" onClick={props.onClose}>取消</Btn>
          <Btn variant="primary" onClick={() => void submit()} disabled={busy()}>{busy() ? '创建中…' : '创建'}</Btn>
        </>
      }
    >
      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
        <Field label="邮箱">
          <input class="input" type="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} placeholder="user@example.com" />
        </Field>
        <Field label="用户名">
          <input class="input" value={username()} onInput={(e) => setUsername(e.currentTarget.value)} placeholder="2-50 字符,字母数字下划线" />
        </Field>
        <Field label="初始密码" hint="将走标准密码强度校验">
          <input class="input" type="text" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} placeholder="至少 8 字符,含大小写字母 + 数字" />
        </Field>
        <Field label="角色">
          <select class="select" value={r()} onChange={(e) => setR(e.currentTarget.value as Role)}>
            <option value="user">普通用户</option>
            <option value="staff">员工</option>
            <option value="admin">管理员</option>
          </select>
        </Field>
      </div>
    </Modal>
  );
}

/* ---------- 用户详情 Drawer ---------- */
type DetailTab = 'profile' | 'sessions' | 'devices' | 'activity' | 'audit';

function UserDrawer(props: {
  user: AdminUser | null;
  onClose: () => void;
  onBan: (u: AdminUser) => void | Promise<void>;
  onRole: (u: AdminUser, r: Role) => void | Promise<void>;
  onReset: (u: AdminUser) => void | Promise<void>;
}) {
  const [tab, setTab] = createSignal<DetailTab>('profile');
  const u = () => props.user;

  // 每次打开新用户回到 profile tab
  createEffect(() => { if (props.user) setTab('profile'); });

  const [profile] = createResource<UserProfile | null, AdminUser | null>(u, async (cur) => (cur ? adminApi.userProfile(cur.id) : null));
  const [extras] = createResource<UserExtras | null, AdminUser | null>(u, async (cur) => (cur ? adminApi.userExtras(cur.id) : null));
  const [sessions] = createResource<UserSessionRow[] | null, AdminUser | null>(u, async (cur) => (cur ? (await adminApi.userSessions(cur.id, 20)).sessions : null));
  const [devices] = createResource<ClientDeviceRow[] | null, AdminUser | null>(u, async (cur) => (cur ? (await adminApi.userDevices(cur.id, 20)).devices : null));
  const [audit] = createResource<AdminAuditEntry[] | null, AdminUser | null>(u, async (cur) => (cur ? (await adminApi.userAuditLog(cur.id, 50)).entries : null));
  const [activity] = createResource<UserActivityEntry[] | null, AdminUser | null>(u, async (cur) => (cur ? (await adminApi.userActivityLog(cur.id, 50)).entries : null));

  return (
    <Drawer
      open={!!props.user}
      onClose={props.onClose}
      title={u()?.username ?? '用户详情'}
      subtitle={u()?.email}
      width={540}
      footer={
        <Show when={u()}>
          {(cur) => (
            <>
              <Btn variant="secondary" icon="key" onClick={() => void props.onReset(cur())}>重置密码</Btn>
              <Btn variant={cur().isBanned ? 'success' : 'danger'} icon="ban" onClick={() => { void props.onBan(cur()); props.onClose(); }}>
                {cur().isBanned ? '解封' : '封禁'}
              </Btn>
            </>
          )}
        </Show>
      }
    >
      <Show when={u()}>
        {(cur) => (
          <>
            <div style={sx({ display: 'flex', gap: 8, marginBottom: 14, alignItems: 'center', flexWrap: 'wrap' })}>
              <Badge variant={ROLE_VARIANT[cur().role]}>{ROLE_LABEL[cur().role] ?? cur().role}</Badge>
              <Badge variant={cur().isBanned ? 'error' : 'success'} dot>{cur().isBanned ? '已封禁' : '正常'}</Badge>
              <select
                class="select"
                style={sx({ width: 'auto', marginLeft: 'auto', padding: '5px 9px', fontSize: 12 })}
                value={cur().role}
                onChange={(e) => void props.onRole(cur(), e.currentTarget.value as Role)}
              >
                <option value="user">设为普通用户</option>
                <option value="staff">设为员工</option>
                <option value="admin">设为管理员</option>
              </select>
            </div>

            <Tabs
              value={tab()}
              onChange={(v) => setTab(v as DetailTab)}
              tabs={[
                { value: 'profile', label: '档案' },
                { value: 'sessions', label: '会话' },
                { value: 'devices', label: '设备' },
                { value: 'activity', label: '活动' },
                { value: 'audit', label: '审计' },
              ]}
            />

            <div style={sx({ marginTop: 14 })}>
              {/* 档案 */}
              <Show when={tab() === 'profile'}>
                <Show when={profile.state !== 'pending'} fallback={<Loading h={160} />}>
                  <Show when={profile()} fallback={<Empty title="暂无档案数据" />}>
                    {(p) => (
                      <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
                        <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 10 })}>
                          <MetricBox label="答题数" value={fmtNum(p().totalRecords)} />
                          <MetricBox label="正确率" value={p().accuracy != null ? (p().accuracy! * 100).toFixed(0) + '%' : '—'} />
                          <MetricBox label="会话数" value={fmtNum(p().sessionCount)} />
                          <MetricBox label="平均反应" value={p().avgResponseTimeMs != null ? Math.round(p().avgResponseTimeMs!) + 'ms' : '—'} />
                          <MetricBox label="ELO" value={extras()?.elo ? Math.round(extras()!.elo!.rating) : '—'} />
                          <MetricBox label="连续天数" value={(extras()?.streakDays ?? 0) + 'd'} />
                        </div>

                        <div>
                          <div class="eyebrow" style={sx({ marginBottom: 8 })}>词库分布</div>
                          <Show when={p().wordbookDistribution.length > 0} fallback={<div class="muted-3" style={sx({ fontSize: 12.5 })}>尚无词书答题记录</div>}>
                            <For each={p().wordbookDistribution}>
                              {(w) => (
                                <div style={sx({ display: 'flex', justifyContent: 'space-between', padding: '7px 0', borderBottom: '1px solid var(--hairline)', fontSize: 13 })}>
                                  <span>{w.name}</span>
                                  <span class="mono muted">{fmtNum(w.recordCount)} 条</span>
                                </div>
                              )}
                            </For>
                          </Show>
                        </div>

                        <Show when={extras()}>
                          {(e) => (
                            <div>
                              <div class="eyebrow" style={sx({ marginBottom: 8 })}>偏好 / 最近策略</div>
                              <div class="muted" style={sx({ fontSize: 12.5, lineHeight: 1.8 })}>
                                <Show when={e().habit}>
                                  {(h) => <>每日目标 {h().dailyGoalWords ?? '—'} 词 · </>}
                                </Show>
                                <Show when={e().preferences}>
                                  {(pref) => <>语言 {pref().language} · 主题 {pref().theme}</>}
                                </Show>
                                <Show when={e().latestStrategy}>
                                  {(s) => (
                                    <>
                                      <br />最近策略 <b class="mono">{strategyText(s().strategy)}</b>
                                      <Show when={e().latestDevice?.osName}> · 设备 {e().latestDevice!.osName}</Show>
                                    </>
                                  )}
                                </Show>
                              </div>
                            </div>
                          )}
                        </Show>
                      </div>
                    )}
                  </Show>
                </Show>
              </Show>

              {/* 会话 */}
              <Show when={tab() === 'sessions'}>
                <Show when={sessions.state !== 'pending'} fallback={<Loading h={160} />}>
                  <SimpleList
                    items={sessions() ?? []}
                    render={(s) => ({
                      title: fmtTime(s.createdAt).slice(5),
                      badge: s.status === 'completed' ? ['success', '完成'] : s.status === 'active' ? ['info', '进行中'] : ['warning', '中断'],
                      meta: `${s.wordsCount ?? 0} 词 · 正确率 ${s.accuracy != null ? (s.accuracy * 100).toFixed(0) : '—'}%`,
                    })}
                  />
                </Show>
              </Show>

              {/* 设备 */}
              <Show when={tab() === 'devices'}>
                <Show when={devices.state !== 'pending'} fallback={<Loading h={160} />}>
                  <SimpleList
                    items={devices() ?? []}
                    render={(d) => ({
                      title: d.platform + (d.appVersion ? ' · v' + d.appVersion : ''),
                      badge: d.isBanned ? ['error', '封禁'] : ['success', '正常'],
                      meta: fmtAgo(d.lastSeenAt),
                      mono: d.deviceId.slice(0, 16),
                    })}
                  />
                </Show>
              </Show>

              {/* 活动 */}
              <Show when={tab() === 'activity'}>
                <Show when={activity.state !== 'pending'} fallback={<Loading h={160} />}>
                  <SimpleList
                    items={activity() ?? []}
                    render={(a) => ({
                      title: a.detailJson ?? a.action,
                      badge: ['default', a.action],
                      meta: fmtAgo(a.createdAt) + (a.ip ? ' · ' + a.ip : ''),
                    })}
                  />
                </Show>
              </Show>

              {/* 审计 */}
              <Show when={tab() === 'audit'}>
                <Show when={audit.state !== 'pending'} fallback={<Loading h={160} />}>
                  <SimpleList
                    items={audit() ?? []}
                    render={(a) => ({
                      title: a.metadataJson ?? a.action,
                      badge: ['accent', a.action],
                      meta: a.adminId.slice(0, 8) + ' · ' + fmtAgo(a.startedAt),
                    })}
                  />
                </Show>
              </Show>
            </div>
          </>
        )}
      </Show>
    </Drawer>
  );
}

/* ---------- helpers ---------- */
function strategyText(strategy: Record<string, unknown>): string {
  const parts: string[] = [];
  if (typeof strategy.algorithm_id === 'string') parts.push(strategy.algorithm_id);
  if (typeof strategy.batch_size === 'number') parts.push(`batch=${strategy.batch_size}`);
  if (typeof strategy.new_ratio === 'number') parts.push(`new=${(strategy.new_ratio as number).toFixed(2)}`);
  if (typeof strategy.difficulty === 'number') parts.push(`diff=${(strategy.difficulty as number).toFixed(2)}`);
  return parts.length > 0 ? parts.join(' · ') : JSON.stringify(strategy).slice(0, 80);
}

function MetricBox(props: { label: string; value: JSX.Element | string | number }) {
  return (
    <div style={sx({ padding: '11px 12px', borderRadius: 11, background: 'var(--surface-sunken)', border: '1px solid var(--hairline)' })}>
      <div class="muted-3" style={sx({ fontSize: 10.5 })}>{props.label}</div>
      <div class="tnum" style={sx({ fontSize: 18, fontWeight: 700, marginTop: 2 })}>{props.value}</div>
    </div>
  );
}

interface RowRender {
  title: string;
  badge?: ['default' | 'accent' | 'success' | 'warning' | 'error' | 'info', string];
  meta?: string;
  mono?: string;
}
function SimpleList<T>(props: { items: T[]; render: (it: T) => RowRender }) {
  return (
    <Show when={props.items.length > 0} fallback={<Empty title="暂无记录" />}>
      <div style={sx({ display: 'flex', flexDirection: 'column' })}>
        <For each={props.items}>
          {(it) => {
            const r = props.render(it);
            return (
              <div style={sx({ display: 'flex', alignItems: 'center', gap: 10, padding: '11px 0', borderBottom: '1px solid var(--hairline)' })}>
                <div style={sx({ minWidth: 0, flex: 1 })}>
                  <div style={sx({ fontWeight: 600, fontSize: 13, wordBreak: 'break-all' })}>{r.title}</div>
                  <Show when={r.mono}><div class="mono muted-3" style={sx({ fontSize: 11 })}>{r.mono}</div></Show>
                  <Show when={r.meta}><div class="muted-3" style={sx({ fontSize: 11.5, marginTop: 2 })}>{r.meta}</div></Show>
                </div>
                <Show when={r.badge}>
                  {(b) => <Badge variant={b()[0]}>{b()[1]}</Badge>}
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </Show>
  );
}
