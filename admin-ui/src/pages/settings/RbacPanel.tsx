import { createSignal, onMount, For, Show } from 'solid-js';
import { adminApi, type RbacAdmin, type AdminRole } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { SectionCard } from './SectionCard';
import { IconRbac } from './icons';

function initials(email: string) {
  const name = email.split('@')[0];
  return name.slice(0, 2).toUpperCase();
}
function fmtTime(s?: string | null) {
  if (!s) return '—';
  const d = new Date(s);
  return isNaN(+d) ? s : d.toLocaleString('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}
function lockState(a: RbacAdmin): { text: string; locked: boolean } {
  if (a.lockedUntil && new Date(a.lockedUntil) > new Date()) return { text: `锁定至 ${fmtTime(a.lockedUntil)}`, locked: true };
  return { text: `创建于 ${fmtTime(a.createdAt)}`, locked: false };
}

export function RbacPanel(props: { ref?: (el: HTMLElement) => void }) {
  const [admins, setAdmins] = createSignal<RbacAdmin[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [showCreate, setShowCreate] = createSignal(false);
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [role, setRole] = createSignal<AdminRole>('admin');
  const [busy, setBusy] = createSignal(false);
  const [delTarget, setDelTarget] = createSignal<RbacAdmin | null>(null);

  async function load() {
    try {
      const res = await adminApi.rbacAdmins();
      setAdmins(res.admins);
    } catch (e) {
      uiStore.toast.error('加载管理员失败', e instanceof Error ? e.message : '');
    } finally {
      setLoading(false);
    }
  }
  onMount(load);

  async function create() {
    if (!email().trim() || !password()) {
      uiStore.toast.warning('邮箱与密码均为必填');
      return;
    }
    setBusy(true);
    try {
      await adminApi.createRbacAdmin({ email: email().trim(), password: password(), role: role() });
      uiStore.toast.success('管理员已创建');
      setShowCreate(false); setEmail(''); setPassword(''); setRole('admin');
      await load();
    } catch (e) {
      uiStore.toast.error('创建失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  }

  async function changeRole(a: RbacAdmin, next: AdminRole) {
    if (a.role === next) return;
    try {
      const updated = await adminApi.updateRbacAdminRole(a.id, next);
      setAdmins((list) => list.map((x) => (x.id === a.id ? updated : x)));
      uiStore.toast.success('角色已更新');
    } catch (e) {
      uiStore.toast.error('更新失败', e instanceof Error ? e.message : '');
    }
  }

  async function doDelete() {
    const a = delTarget();
    if (!a) return;
    try {
      await adminApi.deleteRbacAdmin(a.id);
      setAdmins((list) => list.filter((x) => x.id !== a.id));
      uiStore.toast.success('管理员已删除');
    } catch (e) {
      uiStore.toast.error('删除失败', e instanceof Error ? e.message : '');
    } finally {
      setDelTarget(null);
    }
  }

  return (
    <>
      <ConfirmDialog
        open={!!delTarget()}
        title="删除管理员"
        message={`确定删除 ${delTarget()?.email} 吗？该操作不可撤销。`}
        confirmText="删除"
        variant="warning"
        onConfirm={doDelete}
        onCancel={() => setDelTarget(null)}
      />
      <SectionCard
        id="rbac"
        ref={props.ref}
        icon={<IconRbac />}
        iconStyle={{ background: 'oklch(96% 0.05 162)', color: 'oklch(40% 0.18 162)' }}
        title="管理员与角色"
        desc="后台访问控制 · super-admin 可管理所有人，admin 受限"
        right={<button class="st-btn st-btn-secondary st-btn-sm" onClick={() => setShowCreate((v) => !v)}>+ 邀请管理员</button>}
      >
        <Show when={showCreate()}>
          <div class="st-row" style={{ 'border-bottom': '1px dashed var(--border-hairline)' }}>
            <div class="lbl"><strong>新增管理员</strong><p>新账号将立即可用，请通过安全渠道转交初始密码。</p></div>
            <div class="ctrl">
              <div class="field-group">
                <input class="st-input is-mono wide" placeholder="email@wordforge.app" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
                <input class="st-input is-mono wide" type="password" placeholder="初始密码" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
                <div class="row">
                  <select class="st-input compact" style={{ width: '130px' }} value={role()} onChange={(e) => setRole(e.currentTarget.value as AdminRole)}>
                    <option value="admin">admin</option>
                    <option value="super_admin">super-admin</option>
                  </select>
                  <button class="st-btn st-btn-primary st-btn-sm" disabled={busy()} onClick={create}>创建</button>
                  <button class="st-btn st-btn-ghost st-btn-sm" onClick={() => setShowCreate(false)}>取消</button>
                </div>
              </div>
            </div>
          </div>
        </Show>

        <Show when={!loading()} fallback={<div class="st-skeleton" style={{ height: '120px' }} />}>
          <div class="role-row head">
            <span />
            <span>邮箱</span>
            <span>角色</span>
            <span>状态</span>
            <span />
          </div>
          <Show when={admins().length} fallback={<p class="helper" style={{ padding: '14px 0' }}>暂无管理员</p>}>
            <For each={admins()}>
              {(a) => (
                <div class="role-row">
                  <div class="avatar">{initials(a.email)}</div>
                  <div class="who"><strong>{a.email.split('@')[0]}</strong><span>{a.email}</span></div>
                  <span>
                    <select
                      class="st-input compact"
                      style={{ width: '120px', height: '28px' }}
                      value={a.role}
                      onChange={(e) => changeRole(a, e.currentTarget.value as AdminRole)}
                    >
                      <option value="admin">admin</option>
                      <option value="super_admin">super-admin</option>
                    </select>
                  </span>
                  <span class="last" style={lockState(a).locked ? { color: 'var(--error-strong)' } : undefined}>{lockState(a).text}</span>
                  <div class="row" style={{ 'justify-content': 'flex-end' }}>
                    <button class="st-btn st-btn-ghost st-btn-sm" onClick={() => setDelTarget(a)}>删除</button>
                  </div>
                </div>
              )}
            </For>
          </Show>
        </Show>
      </SectionCard>
    </>
  );
}
