import { createSignal, createMemo, onMount, onCleanup, For, Show } from 'solid-js';
import { createStore, reconcile, unwrap } from 'solid-js/store';
import { adminApi, type SettingsSection, type SettingsSnapshotMeta } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import './settings/settings.css';
import { SettingsRail, type RailItem } from './settings/SettingsRail';
import { SectionCard, Row, Field } from './settings/SectionCard';
import {
  SiteRenderer, AuthRenderer, RatelimitRenderer, AuditRenderer, BackupRenderer,
  type SectionEditor,
} from './settings/SectionRenderers';
import { RbacPanel } from './settings/RbacPanel';
import { ApiKeysPanel } from './settings/ApiKeysPanel';
import { MaintenanceBanner } from './settings/MaintenanceBanner';
import {
  IconSite, IconAuth, IconRbac,
  IconLimits, IconAudit, IconBackup, IconKeys, IconCheck, IconRollback, IconDownload,
} from './settings/icons';

/* ───────── 板块元信息 ─────────
 * section 后端用 `section: string` 标识；config 板块 5 个 + RBAC / API 密钥 2 个动态面板。
 * 后端 section key 命名可能差异（site/auth/ratelimit/audit/backup 或 limits/audit-config/
 * backup-policy 等），这里用 candidates 做容错匹配。
 */
interface SectionMeta {
  key: string;            // 主 section key
  candidates: string[];   // 后端可能返回的别名
  group: string;
  icon: () => any;
  iconStyle?: Record<string, string>;
  title: string;
  desc: string;
}
const META: SectionMeta[] = [
  { key: 'site', candidates: ['site'], group: '基础', icon: IconSite, iconStyle: { background: 'var(--accent-light)', color: 'var(--accent)' }, title: '站点与外观', desc: '基础信息、品牌、时区,影响所有前端和邮件模板' },
  { key: 'auth', candidates: ['auth'], group: '基础', icon: IconAuth, iconStyle: { background: 'oklch(96% 0.04 213)', color: 'oklch(40% 0.18 213)' }, title: '认证与登录', desc: 'JWT、第三方登录、2FA、会话策略' },
  { key: 'ratelimit', candidates: ['ratelimit', 'limits'], group: '基础设施', icon: IconLimits, iconStyle: { background: 'oklch(96% 0.04 28)', color: 'oklch(54% 0.22 27)' }, title: '速率限制', desc: '分用户 / 分接口 / 全局 · token bucket 算法' },
  { key: 'audit', candidates: ['audit', 'audit-config'], group: '合规', icon: IconAudit, iconStyle: { background: 'oklch(96% 0.04 162)', color: 'oklch(40% 0.18 162)' }, title: '审计日志', desc: '所有管理后台操作 · 不可篡改 · 哈希链可验证' },
  { key: 'backup', candidates: ['backup', 'backup-policy'], group: '合规', icon: IconBackup, iconStyle: { background: 'oklch(96% 0.04 269)', color: 'var(--accent)' }, title: '备份策略', desc: 'PostgreSQL + 文件存储 + 配置仓库 · WAL 流式归档' },
];

// 左侧 rail 顺序：5 config + rbac + apikeys（rbac 在 auth 后、apikeys 收尾）
const RAIL: { key: string; group: string; icon: () => any; label: string }[] = [
  { key: 'site', group: '基础', icon: IconSite, label: '站点与外观' },
  { key: 'auth', group: '基础', icon: IconAuth, label: '认证与登录' },
  { key: 'rbac', group: '基础', icon: IconRbac, label: '管理员与角色' },
  { key: 'ratelimit', group: '基础设施', icon: IconLimits, label: '速率限制' },
  { key: 'audit', group: '合规', icon: IconAudit, label: '审计日志' },
  { key: 'backup', group: '合规', icon: IconBackup, label: '备份策略' },
  { key: 'apikeys', group: '开放', icon: IconKeys, label: 'API 密钥' },
];
const RAIL_ORDER = RAIL.map((r) => r.key);
const labelOf = (k: string) => RAIL.find((r) => r.key === k)?.label ?? k;

function humanize(k: string) {
  return k.replace(/[_-]/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2');
}
function isSecret(k: string) {
  return /secret|password|token|key|dsn|access[_-]?key/i.test(k);
}
function relTime(s?: string) {
  if (!s) return '';
  const d = new Date(s);
  if (isNaN(+d)) return s;
  const diff = Date.now() - +d;
  const h = Math.floor(diff / 3600000);
  if (h < 1) return `${Math.max(1, Math.floor(diff / 60000))} 分钟前`;
  if (h < 24) return `${h} 小时前`;
  return `${Math.floor(h / 24)} 天前`;
}
// 后端遮蔽密钥为 ••••••：保存时若未改动不回传，避免把掩码写回
function isMasked(v: unknown) {
  return typeof v === 'string' && /^[•*]{3,}$/.test(v.trim());
}

export default function SettingsPage() {
  const [loading, setLoading] = createSignal(true);
  // 各板块基线（服务器值，按 META.key 归一化）与脏 patch
  const [baseline, setBaseline] = createStore<Record<string, SettingsSection>>({});
  const [patch, setPatch] = createStore<Record<string, Record<string, unknown>>>({});
  const [activeKey, setActiveKey] = createSignal<string>('site');
  const [savingAll, setSavingAll] = createSignal(false);
  const [showRevert, setShowRevert] = createSignal(false);
  const [snapshotting, setSnapshotting] = createSignal(false);
  const [apiKeysBadge, setApiKeysBadge] = createSignal<{ text: string; tone: 'warn' } | undefined>();

  const [maintenance, setMaintenance] = createSignal(false);
  const [lastMaintenance, setLastMaintenance] = createSignal<string | undefined>();
  const [latestUpdatedAt, setLatestUpdatedAt] = createSignal<string | undefined>();

  const sectionRefs: Record<string, HTMLElement> = {};
  let scrollTarget = false;

  // 把后端 sections 数组（section/json/updatedAt）按 META.key 归一化进 baseline
  function ingest(sections: SettingsSection[]) {
    const base: Record<string, SettingsSection> = {};
    let latest = '';
    for (const m of META) {
      const found = sections.find((s) => m.candidates.includes(s.section));
      if (found) {
        base[m.key] = found;
        if (found.updatedAt > latest) latest = found.updatedAt;
      } else {
        base[m.key] = { section: m.key, json: {}, updatedAt: '' };
      }
    }
    setBaseline(reconcile(base));
    setPatch(reconcile({}));
    if (latest) setLatestUpdatedAt(latest);
    // 维护模式：site.json.maintenanceMode（容错）
    const site = base['site']?.json ?? {};
    if (typeof site['maintenanceMode'] === 'boolean') setMaintenance(site['maintenanceMode'] as boolean);
    if (typeof site['lastMaintenance'] === 'string') setLastMaintenance(site['lastMaintenance'] as string);
  }

  async function load() {
    try {
      const cfg = await adminApi.settingsConfig();
      ingest(cfg.sections);
    } catch (e) {
      uiStore.toast.error('加载配置失败', e instanceof Error ? e.message : '');
    } finally {
      setLoading(false);
    }
    // 维护模式兜底：legacy settings 端点
    if (typeof baseline['site']?.json?.['maintenanceMode'] !== 'boolean') {
      try {
        const legacy = await adminApi.getSettings();
        if (typeof legacy['maintenanceMode'] === 'boolean') setMaintenance(legacy['maintenanceMode'] as boolean);
      } catch { /* 忽略：维护横幅按默认 false 展示 */ }
    }
  }

  onMount(() => {
    void load();
    window.addEventListener('beforeunload', beforeUnload);
    window.addEventListener('keydown', onKeydown);
    window.addEventListener('scroll', onScroll, { passive: true });
  });
  onCleanup(() => {
    window.removeEventListener('beforeunload', beforeUnload);
    window.removeEventListener('keydown', onKeydown);
    window.removeEventListener('scroll', onScroll);
  });

  function beforeUnload(e: BeforeUnloadEvent) {
    if (dirtyKeys().length) { e.preventDefault(); e.returnValue = ''; }
  }
  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') {
      e.preventDefault();
      if (dirtyKeys().length) void saveAll();
    }
  }
  function onScroll() {
    if (scrollTarget) return;
    let cur = activeKey();
    for (const k of RAIL_ORDER) {
      const el = sectionRefs[k];
      if (el && el.getBoundingClientRect().top <= 120) cur = k;
    }
    if (cur !== activeKey()) setActiveKey(cur);
  }

  function fieldValue(sec: string, key: string): unknown {
    const p = patch[sec];
    if (p && key in p) return p[key];
    return baseline[sec]?.json?.[key];
  }
  function setField(sec: string, key: string, val: unknown) {
    setPatch(sec, (prev) => ({ ...(prev ?? {}), [key]: val }));
  }
  function sectionDirty(sec: string): number {
    const p = patch[sec];
    const b = baseline[sec]?.json ?? {};
    if (!p) return 0;
    return Object.keys(p).filter((k) => JSON.stringify(p[k]) !== JSON.stringify(b[k])).length;
  }
  const dirtyKeys = createMemo(() => RAIL_ORDER.filter((k) => sectionDirty(k) > 0));
  const totalDirty = createMemo(() => dirtyKeys().reduce((n, k) => n + sectionDirty(k), 0));

  async function saveSection(sec: string): Promise<boolean> {
    if (sectionDirty(sec) === 0) return true;
    const b = baseline[sec]?.json ?? {};
    const p = unwrap(patch[sec] ?? {});
    // 合并：跳过未改动的被遮蔽密钥（避免把 •••••• 写回覆盖真实值）
    const merged: Record<string, unknown> = {};
    for (const [k, v] of Object.entries({ ...b, ...p })) {
      const changed = JSON.stringify(p[k]) !== JSON.stringify(b[k]) && k in p;
      if (isMasked(v) && !changed) continue;
      merged[k] = v;
    }
    try {
      const updated = await adminApi.putSettingsSection(baseline[sec]?.section || sec, merged);
      setBaseline(sec, reconcile(updated));
      setPatch(sec, reconcile({}));
      if (updated.updatedAt) setLatestUpdatedAt(updated.updatedAt);
      return true;
    } catch (e) {
      uiStore.toast.error(`保存「${labelOf(sec)}」失败`, e instanceof Error ? e.message : '');
      return false;
    }
  }

  async function saveAll() {
    setSavingAll(true);
    let ok = true;
    for (const k of dirtyKeys()) ok = (await saveSection(k)) && ok;
    setSavingAll(false);
    if (ok) uiStore.toast.success('全部设置已保存');
  }

  function revertAll() {
    setPatch(reconcile({}));
    setShowRevert(false);
    uiStore.toast.success('已放弃未保存修改');
  }

  function jump(key: string) {
    setActiveKey(key);
    scrollTarget = true;
    sectionRefs[key]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    setTimeout(() => { scrollTarget = false; }, 600);
  }

  async function createSnapshot() {
    setSnapshotting(true);
    try {
      const snap = await adminApi.createSettingsSnapshot();
      uiStore.toast.success(`已创建快照 #${snap.id}`, snap.label ?? undefined);
    } catch (e) {
      uiStore.toast.error('创建快照失败', e instanceof Error ? e.message : '');
    } finally {
      setSnapshotting(false);
    }
  }

  const [showRollback, setShowRollback] = createSignal(false);
  const [snapshots, setSnapshots] = createSignal<SettingsSnapshotMeta[]>([]);
  const [rolling, setRolling] = createSignal(false);
  async function openRollback() {
    setShowRollback(true);
    try {
      const res = await adminApi.settingsSnapshots();
      setSnapshots(res.snapshots);
    } catch (e) {
      uiStore.toast.error('加载快照失败', e instanceof Error ? e.message : '');
    }
  }
  async function restore(id: number) {
    setRolling(true);
    try {
      const cfg = await adminApi.restoreSettingsSnapshot(id);
      ingest(cfg.sections);
      setShowRollback(false);
      uiStore.toast.success(`已回滚到快照 #${id}`);
    } catch (e) {
      uiStore.toast.error('回滚失败', e instanceof Error ? e.message : '');
    } finally {
      setRolling(false);
    }
  }

  async function exportToml() {
    try {
      const toml = await adminApi.exportSettingsToml();
      const blob = new Blob([toml], { type: 'application/toml' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url; a.download = `wordforge-settings-${Date.now()}.toml`;
      a.click();
      URL.revokeObjectURL(url);
      uiStore.toast.success('已导出 settings.toml');
    } catch (e) {
      uiStore.toast.error('导出失败', e instanceof Error ? e.message : '');
    }
  }

  // 健康：7 板块（5 config 当前实现恒 ok + rbac + apikeys）。后端无 per-section health 字段时统一 ok。
  const healthPassed = createMemo(() => 7);
  const railItems = createMemo<RailItem[]>(() =>
    RAIL.map((r) => {
      const dirty = sectionDirty(r.key);
      let badge: RailItem['badge'];
      if (r.key === 'apikeys' && apiKeysBadge()) badge = apiKeysBadge();
      else if (dirty > 0) badge = { text: `${dirty} 待存`, tone: 'warn' };
      return { key: r.key, label: r.label, group: r.group, icon: r.icon, badge };
    }),
  );

  function rightStatus(m: SectionMeta) {
    if (sectionDirty(m.key) > 0) return undefined; // 由 dirtyCount 渲染
    const upd = baseline[m.key]?.updatedAt;
    return <span>{upd ? `更新于 ${relTime(upd)}` : '已同步'}</span>;
  }

  function renderFields(m: SectionMeta) {
    const fields = baseline[m.key]?.json ?? {};
    const keys = Object.keys(fields).filter((k) => k !== 'maintenanceMode' && k !== 'lastMaintenance');
    if (keys.length === 0) {
      return <p class="helper" style={{ padding: '8px 0' }}>该板块暂无可编辑字段（后端未下发该 section）。</p>;
    }
    return (
      <For each={keys}>
        {(key) => {
          const v = fields[key];
          const secret = isSecret(key);
          if (typeof v === 'boolean') {
            return (
              <Row label={humanize(key)} metaPill={secret ? 'SECRET' : undefined}>
                <label class="st-switch">
                  <input type="checkbox" checked={fieldValue(m.key, key) as boolean} onChange={(e) => setField(m.key, key, e.currentTarget.checked)} />
                  <span class="slider" />
                </label>
              </Row>
            );
          }
          const numeric = typeof v === 'number';
          return (
            <Row label={humanize(key)} metaPill={secret ? 'SECRET' : undefined}>
              <Field
                value={fieldValue(m.key, key)}
                mono={secret || /url|dsn|host|cron|key|id/i.test(key)}
                type={secret ? 'password' : numeric ? 'number' : 'text'}
                size={numeric ? 'compact' : 'wide'}
                onChange={(val) => setField(m.key, key, numeric ? (val === '' ? '' : Number(val)) : val)}
              />
            </Row>
          );
        }}
      </For>
    );
  }

  // typed section 编辑上下文:value 返回合并后实时值;onPatch 把完整对象写回 patch。
  // 一旦 patch 存在即视为完整 section 体(后端 strict serde,缺字段会 422)。
  function sectionEditor(key: string): SectionEditor {
    return {
      value: () => {
        const p = patch[key];
        if (p && Object.keys(p).length) return p as Record<string, unknown>;
        return (baseline[key]?.json ?? {}) as Record<string, unknown>;
      },
      onPatch: (next) => setPatch(key, reconcile(next)),
    };
  }

  // 板块 key → 专属渲染器;未映射的回退到通用 renderFields。
  const TYPED_RENDERERS: Record<string, (e: SectionEditor) => any> = {
    site: SiteRenderer, auth: AuthRenderer, ratelimit: RatelimitRenderer,
    audit: AuditRenderer, backup: BackupRenderer,
  };
  function renderSection(key: string) {
    const R = TYPED_RENDERERS[key];
    if (R) return R(sectionEditor(key));
    return renderFields(metaOf(key));
  }

  function sectionProps(m: SectionMeta) {
    return {
      id: m.key,
      icon: m.icon(),
      iconStyle: m.iconStyle,
      title: m.title,
      desc: m.desc,
      right: rightStatus(m),
      ref: (el: HTMLElement) => { sectionRefs[m.key] = el; },
    };
  }
  const metaOf = (k: string) => META.find((x) => x.key === k)!;

  return (
    <div class="st-page">
      <div class="st-page-header">
        <div class="row spread">
          <div>
            <h1 class="st-page-title">系统设置</h1>
            <p class="st-page-desc">
              7 个配置板块 · 全部修改写入 audit_log,支持回滚到任意快照。涉及敏感字段（JWT secret）需 super-admin 二次确认。
            </p>
          </div>
          <div class="st-page-actions">
            <button class="st-btn st-btn-secondary" onClick={openRollback}><IconRollback /> 回滚到快照</button>
            <button class="st-btn st-btn-secondary" disabled={snapshotting()} onClick={createSnapshot}><IconCheck /> 创建快照</button>
            <button class="st-btn st-btn-secondary" onClick={exportToml}><IconDownload /> 导出 .toml</button>
            <button class="st-btn st-btn-primary" disabled={!dirtyKeys().length || savingAll()} onClick={saveAll}><IconCheck /> 全部保存</button>
          </div>
        </div>
      </div>

      <MaintenanceBanner active={maintenance()} lastMaintenance={lastMaintenance()} onChange={setMaintenance} />

      <div class="st-layout">
        <SettingsRail
          items={railItems()}
          active={activeKey()}
          health="ok"
          healthPassed={healthPassed()}
          healthTotal={7}
          rev={latestUpdatedAt() ? `config-rev · ${relTime(latestUpdatedAt())}` : '加载中…'}
          onJump={jump}
        />

        <div>
          <Show when={!loading()} fallback={
            <div style={{ display: 'grid', gap: '18px' }}>
              <div class="st-skeleton" style={{ height: '240px' }} />
              <div class="st-skeleton" style={{ height: '240px' }} />
            </div>
          }>
            <SectionCard {...sectionProps(metaOf('site'))} dirtyCount={sectionDirty('site') || undefined}>
              {renderSection('site')}
            </SectionCard>
            <SectionCard {...sectionProps(metaOf('auth'))} dirtyCount={sectionDirty('auth') || undefined}>
              {renderSection('auth')}
            </SectionCard>

            <RbacPanel ref={(el) => (sectionRefs['rbac'] = el)} />

            <For each={['ratelimit', 'audit', 'backup']}>
              {(k) => (
                <SectionCard {...sectionProps(metaOf(k))} dirtyCount={sectionDirty(k) || undefined}>
                  {renderSection(k)}
                </SectionCard>
              )}
            </For>

            <ApiKeysPanel
              ref={(el) => (sectionRefs['apikeys'] = el)}
              onExpiringChange={(n) => setApiKeysBadge(n > 0 ? { text: `${n} 即将过期`, tone: 'warn' } : undefined)}
            />
          </Show>
        </div>
      </div>

      <div class={`st-save-bar ${dirtyKeys().length ? 'is-show' : ''}`}>
        <div class="summary">
          <b>{totalDirty()}</b> 处未保存修改
          <span>{dirtyKeys().map((k) => labelOf(k)).join(' · ')}</span>
        </div>
        <button class="revert" onClick={() => setShowRevert(true)}>放弃</button>
        <button class="save" disabled={savingAll()} onClick={saveAll}>
          <IconCheck /> <span style={{ 'margin-left': '6px' }}>保存修改 ⌘S</span>
        </button>
      </div>

      <ConfirmDialog
        open={showRevert()}
        title="放弃未保存修改"
        message={`将丢弃 ${totalDirty()} 处未保存的配置修改，确定继续吗？`}
        confirmText="放弃修改"
        variant="warning"
        onConfirm={revertAll}
        onCancel={() => setShowRevert(false)}
      />

      <Show when={showRollback()}>
        <div class="st-modal-overlay" style={overlayStyle} onClick={() => setShowRollback(false)}>
          <div class="st-section" style={{ width: '520px', 'max-width': '92vw', margin: '0' }} onClick={(e) => e.stopPropagation()}>
            <div class="sec-head">
              <div class="ic"><IconRollback /></div>
              <div><div class="title">回滚到历史快照</div><div class="desc">恢复后当前未保存修改将被丢弃</div></div>
            </div>
            <div class="sec-body">
              <Show when={snapshots().length} fallback={<p class="helper">暂无可用快照，先在页头「创建快照」。</p>}>
                <For each={snapshots()}>
                  {(s) => (
                    <div class="role-row" style={{ 'grid-template-columns': 'minmax(0,1fr) 90px' }}>
                      <div class="who">
                        <strong>{s.label || `快照 #${s.id}`}</strong>
                        <span>{relTime(s.createdAt)}</span>
                      </div>
                      <div class="row" style={{ 'justify-content': 'flex-end' }}>
                        <button class="st-btn st-btn-secondary st-btn-sm" disabled={rolling()} onClick={() => restore(s.id)}>恢复</button>
                      </div>
                    </div>
                  )}
                </For>
              </Show>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}

const overlayStyle: Record<string, string> = {
  position: 'fixed', inset: '0', background: 'rgba(15,23,42,0.45)',
  display: 'grid', 'place-items': 'center', 'z-index': '50', padding: '20px',
};
