import { createSignal, createResource, createEffect, For, Show, type JSX } from 'solid-js';
import {
  adminApi,
  type SettingsSection,
  type SettingsConfigResponse,
  type SettingsSnapshotMeta,
  type RbacAdmin,
  type AdminRole,
  type ApiKey,
  type ApiKeyScope,
} from '@/api/admin';
import type { BackupTargetStatus, VersionGate } from '@/types/admin';
import {
  Btn, IconBtn, Badge, Card, Panel, Field, Switch, Tabs, Seg, Empty, Loading,
  Modal, Confirm, Icon, PageHead, fmtBytes, fmtAgo, fmtTime,
} from '@/components/wf';
import { sx } from '@/components/wf';
import { toast } from '@/components/wf';

/* ============================================================
   系统设置 (settings)
   站点 / 认证 / 速率 / 审计 / 备份 等全局配置 · RBAC 管理员 · API 密钥 · 配置快照
   ============================================================ */

// section key → 中文标签（后端 section key 命名可能有别名，做容错映射）
const SECTION_LABEL: Record<string, string> = {
  site: '站点与外观',
  auth: '认证与登录',
  ratelimit: '速率限制',
  roles: '角色权限',
  audit: '审计日志',
  'audit-config': '审计日志',
  backup: '备份策略',
  'backup-policy': '备份策略',
  limits: '资源配额',
};
const sectionLabel = (sec: string) => SECTION_LABEL[sec] ?? sec;

// 把 sections 分组到左侧导航（按后端实际返回的 section 动态归组）
const GROUP_OF: Record<string, string> = {
  site: '站点',
  auth: '认证 · 安全', ratelimit: '认证 · 安全', roles: '认证 · 安全',
  audit: '审计 · 备份', 'audit-config': '审计 · 备份', backup: '审计 · 备份', 'backup-policy': '审计 · 备份',
  limits: '配额',
};
const GROUP_ORDER = ['站点', '认证 · 安全', '审计 · 备份', '配额', '其他'];
const groupOf = (sec: string) => GROUP_OF[sec] ?? '其他';

// 枚举字段 → 可选项（与后端 serde 枚举对齐），让非技术用户下拉选择而非手敲
const FIELD_ENUMS: Record<string, string[]> = {
  theme: ['light', 'dark', 'system'],
  'jwt.algorithm': ['HS256', 'RS256', 'ES256'],
  'registration.policy': ['open', 'invite_only', 'enterprise_domain', 'closed'],
  'twoFactor.admin': ['off', 'optional', 'required'],
  'twoFactor.regular': ['off', 'optional', 'required'],
  'siem.target': ['none', 'elk', 'datadog', 'splunk'],
};

// 后端遮蔽密钥为 ••••••：保存时若未改动不回传，避免把掩码写回覆盖真实值
function isMasked(v: unknown) {
  return typeof v === 'string' && /^[•*]{3,}$/.test(v.trim());
}

type Tab = 'config' | 'rbac' | 'keys' | 'snapshots' | 'maintenance';

export default function SettingsPage() {
  const [tab, setTab] = createSignal<Tab>('config');

  const [config, { refetch: reloadConfig }] = createResource<SettingsConfigResponse>(() => adminApi.settingsConfig());
  const [snapshots, { refetch: reloadSnaps }] = createResource(() => adminApi.settingsSnapshots());
  const [admins, { refetch: reloadAdmins }] = createResource(() => adminApi.rbacAdmins());
  const [keys, { refetch: reloadKeys }] = createResource(() => adminApi.apiKeys());

  const [activeSection, setActiveSection] = createSignal<string>('');

  const sections = (): SettingsSection[] => config()?.sections ?? [];
  // 首个 section 兜底高亮
  createEffect(() => {
    const list = sections();
    if (list.length && !list.some((s) => s.section === activeSection())) {
      setActiveSection(list[0].section);
    }
  });
  const active = () => sections().find((s) => s.section === activeSection());

  // 左侧分组导航
  const grouped = () => {
    const map = new Map<string, SettingsSection[]>();
    for (const s of sections()) {
      const g = groupOf(s.section);
      if (!map.has(g)) map.set(g, []);
      map.get(g)!.push(s);
    }
    return GROUP_ORDER
      .filter((g) => map.has(g))
      .map((g) => ({ label: g, items: map.get(g)! }));
  };

  async function exportToml() {
    try {
      const t = await adminApi.exportSettingsToml();
      const blob = new Blob([t], { type: 'application/toml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `wordforge-settings-${Date.now()}.toml`;
      a.click();
      URL.revokeObjectURL(url);
      toast.success('已导出 settings.toml');
    } catch (e) {
      toast.error('导出失败', e instanceof Error ? e.message : '');
    }
  }

  return (
    <div>
      <PageHead
        title="系统设置"
        desc="站点、认证、速率、审计、备份等全局配置，RBAC 管理员、API 密钥与配置快照。"
        right={<Btn variant="secondary" icon="download" onClick={exportToml}>导出 TOML</Btn>}
      />

      <Tabs
        value={tab()}
        onChange={(v) => setTab(v as Tab)}
        tabs={[
          { value: 'config', label: '配置板块' },
          { value: 'rbac', label: '管理员 RBAC', count: admins()?.admins.length ?? null },
          { value: 'keys', label: 'API 密钥', count: keys()?.keys.length ?? null },
          { value: 'snapshots', label: '快照', count: snapshots()?.snapshots.length ?? null },
          { value: 'maintenance', label: '维护 · 备份' },
        ]}
      />

      <div style={sx({ marginTop: 16 })}>
        {/* ───────── 配置板块 ───────── */}
        <Show when={tab() === 'config'}>
          <Show when={config.state !== 'pending'} fallback={<Card><Loading /></Card>}>
            <Show when={!config.error} fallback={<Card><Empty title="加载配置失败" desc="请检查管理员令牌或网络后重试。" icon="alert" /></Card>}>
              <Show when={sections().length} fallback={<Card><Empty title="暂无配置板块" desc="后端未下发任何 section。" /></Card>}>
                <div style={sx({ display: 'grid', gridTemplateColumns: '220px 1fr', gap: 16 })} class="grid-collapse">
                  <Card pad={false} style={{ overflow: 'hidden', 'align-self': 'start' }}>
                    <For each={grouped()}>
                      {(g) => (
                        <div>
                          <div class="eyebrow" style={sx({ padding: '12px 14px 6px' })}>{g.label}</div>
                          <For each={g.items}>
                            {(s) => (
                              <button
                                onClick={() => setActiveSection(s.section)}
                                style={sx({
                                  width: '100%', textAlign: 'left', padding: '9px 16px', border: 'none',
                                  cursor: 'pointer', fontSize: 13,
                                  fontWeight: activeSection() === s.section ? 600 : 500,
                                  color: activeSection() === s.section ? 'var(--accent)' : 'var(--text-2)',
                                  background: activeSection() === s.section ? 'var(--accent-soft)' : 'transparent',
                                })}
                              >
                                {sectionLabel(s.section)}
                              </button>
                            )}
                          </For>
                        </div>
                      )}
                    </For>
                  </Card>
                  <Show when={active()} fallback={<Card><Empty title="选择左侧板块" /></Card>}>
                    {(sec) => <SectionEditor section={sec()} onSaved={() => reloadConfig()} />}
                  </Show>
                </div>
              </Show>
            </Show>
          </Show>
        </Show>

        {/* ───────── 管理员 RBAC ───────── */}
        <Show when={tab() === 'rbac'}>
          <Show when={admins.state !== 'pending'} fallback={<Loading />}>
            <RbacPanel admins={admins()?.admins ?? []} onReload={() => reloadAdmins()} />
          </Show>
        </Show>

        {/* ───────── API 密钥 ───────── */}
        <Show when={tab() === 'keys'}>
          <Show when={keys.state !== 'pending'} fallback={<Loading />}>
            <ApiKeysPanel keys={keys()?.keys ?? []} onReload={() => reloadKeys()} />
          </Show>
        </Show>

        {/* ───────── 快照 ───────── */}
        <Show when={tab() === 'snapshots'}>
          <Show when={snapshots.state !== 'pending'} fallback={<Loading />}>
            <SnapshotsPanel
              snapshots={snapshots()?.snapshots ?? []}
              onReload={() => reloadSnaps()}
              onRestored={() => reloadConfig()}
            />
          </Show>
        </Show>

        {/* ───────── 维护 · 备份 ───────── */}
        <Show when={tab() === 'maintenance'}>
          <MaintenancePanel />
        </Show>
      </div>
    </div>
  );
}

/* ---------------------------- 板块编辑器 ---------------------------- */
function SectionEditor(props: { section: SettingsSection; onSaved: () => void }) {
  const [draft, setDraft] = createSignal<Record<string, unknown>>({ ...props.section.json });
  const [busy, setBusy] = createSignal(false);

  // section 切换时重置草稿
  createEffect(() => {
    props.section.section; // 依赖追踪
    setDraft({ ...props.section.json });
  });

  const dirty = () => JSON.stringify(draft()) !== JSON.stringify(props.section.json);
  const setVal = (k: string, v: unknown) => setDraft((d) => ({ ...d, [k]: v }));

  const save = async () => {
    setBusy(true);
    try {
      // 合并：跳过未改动的被遮蔽密钥（避免把 •••••• 写回覆盖真实值）
      const d = draft();
      const base = props.section.json;
      const merged: Record<string, unknown> = {};
      for (const [k, v] of Object.entries(d)) {
        const changed = JSON.stringify(v) !== JSON.stringify(base[k]);
        if (isMasked(v) && !changed) continue;
        merged[k] = v;
      }
      await adminApi.putSettingsSection(props.section.section, merged);
      toast.success(`${sectionLabel(props.section.section)} 配置已保存`);
      props.onSaved();
    } catch (e) {
      toast.error('保存失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  };

  const entries = () => Object.entries(draft());

  return (
    <Card>
      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 })}>
        <div>
          <h2 class="section-title">{sectionLabel(props.section.section)} 配置</h2>
          <div class="muted-3 mono" style={sx({ fontSize: 11, marginTop: 3 })}>
            section: {props.section.section}
            <Show when={props.section.updatedAt}> · 更新于 {fmtAgo(props.section.updatedAt)}</Show>
          </div>
        </div>
        <Show when={dirty()}><Badge variant="warning" dot>未保存</Badge></Show>
      </div>

      <Show
        when={entries().length}
        fallback={<p class="muted" style={sx({ fontSize: 13, padding: '8px 0' })}>该板块暂无可编辑字段（后端未下发该 section）。</p>}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
          <For each={entries()}>
            {([k, v]) => {
              const masked = isMasked(v);
              const opts = FIELD_ENUMS[k];
              return (
                <div style={sx({ display: 'grid', gridTemplateColumns: '210px 1fr', gap: 14, alignItems: 'center' })}>
                  <div>
                    <div style={sx({ fontSize: 13, fontWeight: 600 })}>{k}</div>
                    <Show when={masked}><div class="muted-3" style={sx({ fontSize: 10.5 })}>已脱敏</div></Show>
                  </div>
                  {opts ? (
                    <select class="select" value={String(v ?? '')} onChange={(e) => setVal(k, e.currentTarget.value)}>
                      <For each={opts}>{(o) => <option value={o}>{o}</option>}</For>
                    </select>
                  ) : typeof v === 'boolean' ? (
                    <Switch checked={v} onChange={(nv) => setVal(k, nv)} />
                  ) : typeof v === 'number' ? (
                    <input
                      class="input" type="number" value={v}
                      onInput={(e) => setVal(k, e.currentTarget.value === '' ? '' : Number(e.currentTarget.value))}
                    />
                  ) : (
                    <input
                      class="input" value={String(v ?? '')}
                      onInput={(e) => setVal(k, e.currentTarget.value)}
                      placeholder={masked ? '输入新值以覆盖' : ''}
                    />
                  )}
                </div>
              );
            }}
          </For>
        </div>
      </Show>

      <div style={sx({ marginTop: 18, display: 'flex', justifyContent: 'flex-end' })}>
        <Btn variant="primary" disabled={!dirty() || busy()} onClick={save}>{busy() ? '保存中…' : '保存配置'}</Btn>
      </div>
    </Card>
  );
}

/* ---------------------------- RBAC 管理员 ---------------------------- */
const ROLE_LABEL: Record<AdminRole, string> = { super_admin: '超级管理员', admin: '管理员' };

function RbacPanel(props: { admins: RbacAdmin[]; onReload: () => void }) {
  const [addOpen, setAddOpen] = createSignal(false);
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [role, setRole] = createSignal<AdminRole>('admin');
  const [busy, setBusy] = createSignal(false);
  const [delTarget, setDelTarget] = createSignal<RbacAdmin | null>(null);

  const create = async () => {
    if (!email() || !password()) { toast.warning('请填写邮箱和密码'); return; }
    setBusy(true);
    try {
      await adminApi.createRbacAdmin({ email: email(), password: password(), role: role() });
      toast.success('已创建管理员 ' + email());
      setAddOpen(false); setEmail(''); setPassword('');
      props.onReload();
    } catch (e) {
      toast.error('创建失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  };
  const changeRole = async (a: RbacAdmin, r: AdminRole) => {
    try {
      await adminApi.updateRbacAdminRole(a.id, r);
      toast.success(a.email + ' 角色改为 ' + ROLE_LABEL[r]);
      props.onReload();
    } catch (e) {
      toast.error('修改角色失败', e instanceof Error ? e.message : '');
    }
  };
  const del = async () => {
    const a = delTarget();
    if (!a) return;
    try {
      await adminApi.deleteRbacAdmin(a.id);
      toast.info('已删除 ' + a.email);
    } catch (e) {
      toast.error('删除失败', e instanceof Error ? e.message : '');
    } finally {
      setDelTarget(null);
      props.onReload();
    }
  };

  return (
    <Card pad={false} style={{ overflow: 'hidden' }}>
      <div style={sx({ padding: 14, display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid var(--hairline)' })}>
        <b style={sx({ fontSize: 14 })}>管理员与角色</b>
        <Btn size="sm" variant="primary" icon="plus" onClick={() => setAddOpen(true)}>添加管理员</Btn>
      </div>
      <Show when={props.admins.length} fallback={<div style={sx({ padding: 8 })}><Empty title="暂无管理员" /></div>}>
        <div style={sx({ overflowX: 'auto' })}>
          <table class="tbl">
            <thead><tr><th>邮箱</th><th>角色</th><th>创建时间</th><th>状态</th><th style={sx({ textAlign: 'right' })}>操作</th></tr></thead>
            <tbody>
              <For each={props.admins}>
                {(a) => (
                  <tr>
                    <td class="mono" style={sx({ fontSize: 12.5 })}>{a.email}</td>
                    <td>
                      <select
                        class="select" style={sx({ width: 'auto', padding: '4px 8px', fontSize: 12 })}
                        value={a.role} onChange={(e) => changeRole(a, e.currentTarget.value as AdminRole)}
                      >
                        <option value="super_admin">超级管理员</option>
                        <option value="admin">管理员</option>
                      </select>
                    </td>
                    <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtTime(a.createdAt).slice(0, 10)}</td>
                    <td><Badge variant={a.lockedUntil ? 'error' : 'success'} dot>{a.lockedUntil ? '已锁定' : '正常'}</Badge></td>
                    <td style={sx({ textAlign: 'right' })}><IconBtn name="trash" title="删除" onClick={() => setDelTarget(a)} /></td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>

      <Modal
        open={addOpen()} onClose={() => setAddOpen(false)} title="添加管理员" size="sm"
        footer={<><Btn variant="ghost" onClick={() => setAddOpen(false)}>取消</Btn><Btn variant="primary" disabled={busy()} onClick={create}>{busy() ? '创建中…' : '创建'}</Btn></>}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
          <Field label="邮箱"><input class="input" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} /></Field>
          <Field label="初始密码"><input class="input" type="password" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} /></Field>
          <Field label="角色">
            <select class="select" value={role()} onChange={(e) => setRole(e.currentTarget.value as AdminRole)}>
              <option value="admin">管理员</option>
              <option value="super_admin">超级管理员</option>
            </select>
          </Field>
        </div>
      </Modal>
      <Confirm
        open={!!delTarget()} onClose={() => setDelTarget(null)} onConfirm={del} danger
        title="删除管理员" body={'确认删除 ' + (delTarget()?.email ?? '') + '？'} confirmText="删除"
      />
    </Card>
  );
}

/* ---------------------------- API 密钥 ---------------------------- */
const SCOPE_META: Record<ApiKeyScope, [string, 'info' | 'warning' | 'error']> = {
  read: ['只读', 'info'],
  write: ['读写', 'warning'],
  admin: ['管理', 'error'],
};

function ApiKeysPanel(props: { keys: ApiKey[]; onReload: () => void }) {
  const [addOpen, setAddOpen] = createSignal(false);
  const [name, setName] = createSignal('');
  const [scope, setScope] = createSignal<ApiKeyScope>('read');
  const [busy, setBusy] = createSignal(false);
  const [plaintext, setPlaintext] = createSignal<string | null>(null);
  const [delTarget, setDelTarget] = createSignal<ApiKey | null>(null);

  const create = async () => {
    if (!name()) { toast.warning('请填写名称'); return; }
    setBusy(true);
    try {
      const r = await adminApi.createApiKey({ name: name(), scope: scope() });
      setPlaintext(r.plaintext);
      setAddOpen(false); setName('');
      props.onReload();
    } catch (e) {
      toast.error('创建失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  };
  const rotate = async (k: ApiKey) => {
    try {
      const r = await adminApi.rotateApiKey(k.id);
      setPlaintext(r.plaintext);
      toast.success('已轮换密钥');
      props.onReload();
    } catch (e) {
      toast.error('轮换失败', e instanceof Error ? e.message : '');
    }
  };
  const del = async () => {
    const k = delTarget();
    if (!k) return;
    try {
      await adminApi.deleteApiKey(k.id);
      toast.info('已撤销 ' + k.name);
    } catch (e) {
      toast.error('撤销失败', e instanceof Error ? e.message : '');
    } finally {
      setDelTarget(null);
      props.onReload();
    }
  };
  const copyKey = () => {
    const p = plaintext();
    if (p) { navigator.clipboard?.writeText(p); toast.success('已复制'); }
  };

  return (
    <Card pad={false} style={{ overflow: 'hidden' }}>
      <div style={sx({ padding: 14, display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid var(--hairline)' })}>
        <b style={sx({ fontSize: 14 })}>API 密钥</b>
        <Btn size="sm" variant="primary" icon="plus" onClick={() => setAddOpen(true)}>创建密钥</Btn>
      </div>
      <Show when={props.keys.length} fallback={<div style={sx({ padding: 8 })}><Empty title="暂无 API 密钥" desc="创建后明文仅展示一次，请妥善保存。" icon="key" /></div>}>
        <div style={sx({ overflowX: 'auto' })}>
          <table class="tbl">
            <thead><tr><th>名称</th><th>权限</th><th>前缀</th><th>最近使用</th><th>过期</th><th>状态</th><th style={sx({ textAlign: 'right' })}>操作</th></tr></thead>
            <tbody>
              <For each={props.keys}>
                {(k) => (
                  <tr>
                    <td style={sx({ fontWeight: 600 })}>{k.name}</td>
                    <td><Badge variant={SCOPE_META[k.scope][1]}>{SCOPE_META[k.scope][0]}</Badge></td>
                    <td class="mono" style={sx({ fontSize: 11.5 })}>{k.prefix}…</td>
                    <td class="muted-3" style={sx({ fontSize: 11.5 })}>{k.lastUsedAt ? fmtAgo(k.lastUsedAt) : '从未'}</td>
                    <td class="muted-3" style={sx({ fontSize: 11.5 })}>{k.expiresAt ? fmtTime(k.expiresAt).slice(0, 10) : '永久'}</td>
                    <td><Badge variant={k.revokedAt ? 'error' : 'success'} dot>{k.revokedAt ? '已撤销' : '有效'}</Badge></td>
                    <td>
                      <div style={sx({ display: 'flex', gap: 5, justifyContent: 'flex-end' })}>
                        <IconBtn name="rotate" title="轮换" size={15} onClick={() => rotate(k)} disabled={!!k.revokedAt} />
                        <IconBtn name="trash" title="撤销" size={15} onClick={() => setDelTarget(k)} disabled={!!k.revokedAt} />
                      </div>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>

      <Modal
        open={addOpen()} onClose={() => setAddOpen(false)} title="创建 API 密钥" size="sm"
        footer={<><Btn variant="ghost" onClick={() => setAddOpen(false)}>取消</Btn><Btn variant="primary" disabled={busy()} onClick={create}>{busy() ? '创建中…' : '创建'}</Btn></>}
      >
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 13 })}>
          <Field label="名称"><input class="input" value={name()} onInput={(e) => setName(e.currentTarget.value)} placeholder="如：监控只读" /></Field>
          <Field label="权限范围">
            <select class="select" value={scope()} onChange={(e) => setScope(e.currentTarget.value as ApiKeyScope)}>
              <option value="read">只读</option>
              <option value="write">读写</option>
              <option value="admin">管理</option>
            </select>
          </Field>
        </div>
      </Modal>

      <Modal
        open={!!plaintext()} onClose={() => setPlaintext(null)} title="密钥已创建" size="sm"
        footer={<Btn variant="primary" icon="copy" onClick={copyKey}>复制密钥</Btn>}
      >
        <p class="muted" style={sx({ fontSize: 13, marginTop: 0 })}>
          <b style={sx({ color: 'var(--warning)' })}>明文仅此一次显示</b>，请立即保存。
        </p>
        <div class="mono" style={sx({ padding: 12, borderRadius: 10, background: 'var(--surface-sunken)', border: '1px solid var(--border)', fontSize: 12, wordBreak: 'break-all' })}>
          {plaintext()}
        </div>
      </Modal>

      <Confirm
        open={!!delTarget()} onClose={() => setDelTarget(null)} onConfirm={del} danger
        title="撤销 API 密钥" body="撤销后使用该密钥的调用将立即失败。" confirmText="撤销"
      />
    </Card>
  );
}

/* ---------------------------- 配置快照 ---------------------------- */
function SnapshotsPanel(props: { snapshots: SettingsSnapshotMeta[]; onReload: () => void; onRestored: () => void }) {
  const [label, setLabel] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [restoreTarget, setRestoreTarget] = createSignal<SettingsSnapshotMeta | null>(null);
  const [rolling, setRolling] = createSignal(false);

  const create = async () => {
    setBusy(true);
    try {
      const snap = await adminApi.createSettingsSnapshot(label() || undefined);
      toast.success(`已创建快照 #${snap.id}`, snap.label ?? undefined);
      setLabel('');
      props.onReload();
    } catch (e) {
      toast.error('创建快照失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  };
  const restore = async () => {
    const s = restoreTarget();
    if (!s) return;
    setRolling(true);
    try {
      await adminApi.restoreSettingsSnapshot(s.id);
      toast.success('已回滚到快照 #' + s.id);
      props.onRestored();
    } catch (e) {
      toast.error('回滚失败', e instanceof Error ? e.message : '');
    } finally {
      setRolling(false);
      setRestoreTarget(null);
    }
  };

  return (
    <div style={sx({ display: 'grid', gridTemplateColumns: 'minmax(0,1.4fr) minmax(0,1fr)', gap: 16 })} class="grid-collapse">
      <Card pad={false} style={{ overflow: 'hidden' }}>
        <div style={sx({ padding: 14, borderBottom: '1px solid var(--hairline)', fontWeight: 700, fontSize: 14 })}>配置快照</div>
        <Show when={props.snapshots.length} fallback={<div style={sx({ padding: 8 })}><Empty title="暂无快照" desc="在右侧创建当前配置的基线快照。" /></div>}>
          <div style={sx({ overflowX: 'auto' })}>
            <table class="tbl">
              <thead><tr><th>#</th><th>标签</th><th>创建时间</th><th style={sx({ textAlign: 'right' })}>操作</th></tr></thead>
              <tbody>
                <For each={props.snapshots}>
                  {(s) => (
                    <tr>
                      <td class="mono">#{s.id}</td>
                      <td><Show when={s.label} fallback={<span class="muted-3">（无标签）</span>}>{s.label}</Show></td>
                      <td class="muted-3" style={sx({ fontSize: 11.5 })}>{fmtTime(s.createdAt)}</td>
                      <td style={sx({ textAlign: 'right' })}><Btn size="xs" variant="outline" icon="rotate" onClick={() => setRestoreTarget(s)}>回滚</Btn></td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </Card>
      <Panel title="创建快照" sub="保存当前全部配置">
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 12 })}>
          <Field label="标签（可选）"><input class="input" value={label()} onInput={(e) => setLabel(e.currentTarget.value)} placeholder="如：上线前基线" /></Field>
          <Btn variant="primary" icon="plus" disabled={busy()} onClick={create}>{busy() ? '创建中…' : '创建快照'}</Btn>
        </div>
      </Panel>
      <Confirm
        open={!!restoreTarget()} onClose={() => setRestoreTarget(null)} onConfirm={restore} loading={rolling()}
        title="回滚到快照" body={'将用快照 #' + (restoreTarget()?.id ?? '') + ' 覆盖当前全部配置。'} confirmText="回滚"
      />
    </div>
  );
}

/* ---------------------------- 维护 · 备份 ---------------------------- */
function MaintenancePanel() {
  const [maintenance, setMaintenance] = createSignal(false);
  const [mInit, setMInit] = createSignal(false);
  const [backup, { refetch: reloadBackup }] = createResource(() => adminApi.getBackupStatus());
  const [gate, { refetch: reloadGate }] = createResource<VersionGate>(() => adminApi.getVersionGate());

  // 维护模式初值：从 legacy settings 读取
  void (async () => {
    try {
      const s = await adminApi.getSettings();
      setMaintenance(!!s.maintenanceMode);
    } catch { /* 默认 false */ } finally { setMInit(true); }
  })();

  const toggleMaintenance = async (v: boolean) => {
    setMaintenance(v);
    try {
      await adminApi.setMaintenance(v);
      toast[v ? 'warning' : 'success'](v ? '维护模式已开启' : '维护模式已关闭');
    } catch (e) {
      setMaintenance(!v);
      toast.error('切换维护模式失败', e instanceof Error ? e.message : '');
    }
  };

  return (
    <div style={sx({ display: 'flex', flexDirection: 'column', gap: 16 })}>
      <div style={sx({ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 })} class="grid-collapse">
        <Panel title="维护模式" sub="开启后非管理员将看到维护页">
          <div style={sx({
            display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '14px 16px',
            borderRadius: 12, background: maintenance() ? 'var(--warning-soft)' : 'var(--surface-sunken)',
          })}>
            <div>
              <div style={sx({ fontWeight: 600, fontSize: 14 })}>维护模式</div>
              <div class="muted-3" style={sx({ fontSize: 12, marginTop: 3 })}>
                {maintenance() ? '已开启 · 用户访问受限' : '关闭 · 服务正常'}
              </div>
            </div>
            <Switch checked={maintenance()} disabled={!mInit()} onChange={toggleMaintenance} />
          </div>
        </Panel>

        <VersionGatePanel gate={gate()} loading={gate.state === 'pending'} onSaved={() => reloadGate()} />
      </div>

      <Panel title="备份目标状态" sub="只读运行时状态" right={<Btn size="sm" variant="ghost" icon="refresh" onClick={() => reloadBackup()}>刷新</Btn>}>
        <Show when={backup.state !== 'pending'} fallback={<Loading h={120} />}>
          <Show
            when={(backup()?.targets.length ?? 0) > 0}
            fallback={<Empty title="未配置备份目标" desc="后端未声明任何离站备份 target。" icon="db" />}
          >
            <div style={sx({ display: 'flex', flexDirection: 'column', gap: 10 })}>
              <For each={backup()?.targets ?? []}>
                {(t: BackupTargetStatus) => {
                  const ok = !t.lastError;
                  return (
                    <div style={sx({ padding: 12, borderRadius: 11, border: '1px solid var(--hairline)' })}>
                      <div style={sx({ display: 'flex', justifyContent: 'space-between', alignItems: 'center' })}>
                        <div style={sx({ display: 'flex', gap: 8, alignItems: 'center', minWidth: 0 })}>
                          <Icon name="db" size={15} style={sx({ color: 'var(--accent)', flex: 'none' })} />
                          <b style={sx({ fontSize: 13 })}>{t.name}</b>
                        </div>
                        <Badge variant={ok ? 'success' : 'error'} dot>{ok ? '成功' : '失败'}</Badge>
                      </div>
                      <div class="muted-3 mono" style={sx({ fontSize: 11, marginTop: 7, wordBreak: 'break-all' })}>
                        {t.uri}
                      </div>
                      <div class="muted-3 mono" style={sx({ fontSize: 11, marginTop: 4 })}>
                        {t.lastBytes != null ? fmtBytes(t.lastBytes) + ' · ' : ''}
                        {ok
                          ? '上次成功 ' + (t.lastOkAt ? fmtAgo(t.lastOkAt) : '从未')
                          : '上次尝试 ' + fmtAgo(t.lastAttemptAt)}
                        {t.lastError ? ' · ' + t.lastError : ''}
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>
        </Show>
      </Panel>
    </div>
  );
}

/* ---------------------------- 版本门控 ---------------------------- */
function VersionGatePanel(props: { gate?: VersionGate; loading: boolean; onSaved: () => void }) {
  const [enabled, setEnabled] = createSignal(false);
  const [minVer, setMinVer] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [touched, setTouched] = createSignal(false);

  // 同步服务器值（仅在用户未编辑时）
  createEffect(() => {
    const g = props.gate;
    if (g && !touched()) {
      setEnabled(g.enabled);
      setMinVer(g.minClientVersion ?? '');
    }
  });

  const save = async () => {
    setBusy(true);
    try {
      await adminApi.setVersionGate({
        enabled: enabled(),
        minClientVersion: minVer().trim() || null,
      });
      toast.success('版本门控已更新');
      setTouched(false);
      props.onSaved();
    } catch (e) {
      toast.error('保存失败', e instanceof Error ? e.message : '');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Panel title="客户端版本门控" sub="低于阈值的客户端将被拒绝接入">
      <Show when={!props.loading} fallback={<Loading h={120} />}>
        <div style={sx({ display: 'flex', flexDirection: 'column', gap: 14 })}>
          <div style={sx({
            display: 'flex', justifyContent: 'space-between', alignItems: 'center',
            padding: '12px 14px', borderRadius: 12, background: 'var(--surface-sunken)',
          })}>
            <div>
              <div style={sx({ fontWeight: 600, fontSize: 14 })}>启用门控</div>
              <div class="muted-3" style={sx({ fontSize: 12, marginTop: 3 })}>
                {enabled() ? '已启用 · 即时生效' : '关闭 · 回落 env 兜底'}
              </div>
            </div>
            <Switch checked={enabled()} onChange={(v) => { setTouched(true); setEnabled(v); }} />
          </div>

          <Field label="最低客户端版本（semver，留空则回落 env）">
            <input
              class="input mono" value={minVer()}
              onInput={(e) => { setTouched(true); setMinVer(e.currentTarget.value); }}
              placeholder="如：1.2.0"
            />
          </Field>

          <Show when={props.gate?.effectiveMinClientVersion || props.gate?.envMinClientVersion}>
            <div class="muted-3 mono" style={sx({ fontSize: 11 })}>
              <Show when={props.gate?.effectiveMinClientVersion}>生效阈值 {props.gate!.effectiveMinClientVersion} · </Show>
              env 兜底 {props.gate?.envMinClientVersion ?? '—'}
              <Show when={props.gate?.strictModeEnabled}> · strict-mode 已开</Show>
            </div>
          </Show>

          <div style={sx({ display: 'flex', justifyContent: 'flex-end' })}>
            <Btn variant="primary" disabled={!touched() || busy()} onClick={save}>{busy() ? '保存中…' : '保存门控'}</Btn>
          </div>
        </div>
      </Show>
    </Panel>
  );
}
