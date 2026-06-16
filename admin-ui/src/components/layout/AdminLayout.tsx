import { type ParentProps, Show, For, createSignal, createMemo, onMount, onCleanup } from 'solid-js';
import { A, useLocation, useNavigate } from '@solidjs/router';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';
import { themeStore } from '@/stores/theme';
import { CommandPalette } from '@/components/ui/CommandPalette';
import { OnboardingTour } from '@/components/onboarding/OnboardingTour';
import { useOnboarding } from '@/components/onboarding/useOnboarding';
import { ClockDriftWarning } from '@/components/admin/ClockDriftWarning';
import { NotificationBell } from '@/components/layout/NotificationBell';
import { Icon } from '@/components/wf/Icon';
import { sx, cx } from '@/components/wf/sx';

/* ── new IA: 7 groups, mapped 1:1 onto the existing /admin routes ── */
interface NavItem { id: string; label: string; icon: string; href: string; exact?: boolean; badge?: string }
const NAV: { group: string; items: NavItem[] }[] = [
  { group: '概览', items: [
    { id: 'dashboard', label: '仪表盘', icon: 'dashboard', href: '/admin', exact: true },
  ] },
  { group: '用户 · 设备', items: [
    { id: 'users', label: '用户管理', icon: 'users', href: '/admin/users' },
    { id: 'devices', label: '设备与遥测', icon: 'devices', href: '/admin/clients' },
  ] },
  { group: '学习引擎 · AMAS', items: [
    { id: 'amas-metrics', label: 'AMAS 指标', icon: 'chart', href: '/admin/amas-metrics' },
    { id: 'amas-config', label: 'AMAS 调参', icon: 'sliders', href: '/admin/amas-config' },
    { id: 'amas-advisor', label: 'LLM 顾问', icon: 'bulb', href: '/admin/amas-advisor' },
  ] },
  { group: '数据 · 内容', items: [
    { id: 'analytics', label: '数据分析', icon: 'analytics', href: '/admin/analytics' },
    { id: 'wordbooks', label: '词库中心', icon: 'book', href: '/admin/wordbook-center' },
  ] },
  { group: '运维监控', items: [
    { id: 'monitoring', label: '系统监控', icon: 'monitor', href: '/admin/monitoring' },
    { id: 'probe', label: '数据探针', icon: 'probe', href: '/admin/probe' },
    { id: 'remote-probe', label: '远程探针', icon: 'remote', href: '/admin/remote-probe' },
  ] },
  { group: '消息 · 反馈', items: [
    { id: 'broadcast', label: '消息推送', icon: 'send', href: '/admin/broadcast', badge: 'HUB' },
    { id: 'feedback', label: '用户反馈', icon: 'inbox', href: '/admin/feedback' },
  ] },
  { group: '系统', items: [
    { id: 'updates', label: '版本更新', icon: 'update', href: '/admin/updates' },
    { id: 'resource-packs', label: '资源包', icon: 'package', href: '/admin/resource-packs' },
    { id: 'settings', label: '系统设置', icon: 'settings', href: '/admin/settings' },
  ] },
];
const ALL_ITEMS = NAV.flatMap((g) => g.items);

export function AdminLayout(props: ParentProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = createSignal(false);
  const [mobileOpen, setMobileOpen] = createSignal(false);
  const [adminEmail, setAdminEmail] = createSignal<string | null>(null);
  const onboarding = useOnboarding();

  const isActive = (it: NavItem) => (it.exact ? location.pathname === it.href : location.pathname.startsWith(it.href));
  const activeItem = createMemo(() => ALL_ITEMS.find((it) => isActive(it)));
  const pageTitle = () => activeItem()?.label ?? '管理后台';

  onMount(async () => {
    try { const res = await adminApi.verifyToken(); setAdminEmail(res.email); } catch { /* silent */ }
    try { const h = await adminApi.health(); onboarding.autoShowIfNeeded(h.version); } catch { /* no version → no tour */ }
  });

  // ⌘K is handled by the mounted CommandPalette; expose a click target for the search buttons.
  const openPalette = () => document.querySelector<HTMLButtonElement>('[data-palette-open]')?.click();

  let logoutBusy = false;
  const doLogout = async () => {
    if (logoutBusy) return;
    logoutBusy = true;
    try { await adminApi.logout(); } catch { uiStore.toast.warning('退出请求失败，已清理本地登录状态'); }
    finally { tokenManager.clearAdminToken(); navigate('/admin/login', { replace: true }); }
  };

  const onKey = (e: KeyboardEvent) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { /* CommandPalette owns ⌘K */ }
  };
  onMount(() => window.addEventListener('keydown', onKey));
  onCleanup(() => window.removeEventListener('keydown', onKey));

  return (
    <div class={cx('wf-admin', themeStore.effective() === 'dark' && '')} data-theme={themeStore.effective()}>
      <div class="app-bg" />

      <div style={sx({ display: 'flex', height: '100vh', position: 'relative', zIndex: 1 })}>
        {/* mobile scrim */}
        <Show when={mobileOpen()}>
          <div
            role="presentation" aria-hidden="true"
            style={sx({ position: 'fixed', inset: 0, zIndex: 40, background: 'rgba(8,10,18,0.45)', backdropFilter: 'blur(2px)' })}
            onClick={() => setMobileOpen(false)}
          />
        </Show>

        {/* Sidebar */}
        <aside
          class="glass wf-sidebar"
          classList={{ 'wf-open': mobileOpen() }}
          style={sx({
            width: collapsed() ? 68 : 'var(--sidebar-w)', flex: 'none', display: 'flex', flexDirection: 'column',
            borderRight: '1px solid var(--border)', transition: 'width .28s var(--ease), transform .28s var(--ease)',
            overflow: 'hidden', zIndex: 45,
          })}
        >
          {/* brand */}
          <div style={sx({ display: 'flex', alignItems: 'center', gap: 11, padding: '18px 16px 15px', borderBottom: '1px solid var(--hairline)' })}>
            <div style={sx({ width: 32, height: 32, borderRadius: 9, flex: 'none', position: 'relative', display: 'grid', placeItems: 'center', background: 'var(--accent)', color: '#fff', fontWeight: 700, fontSize: 15, letterSpacing: '-0.04em', fontFamily: 'var(--mono)' })}>
              W
              <span style={sx({ position: 'absolute', inset: 0, borderRadius: 9, boxShadow: 'inset 0 1px 0 rgba(255,255,255,0.28)', pointerEvents: 'none' })} />
            </div>
            <Show when={!collapsed()}>
              <div style={sx({ minWidth: 0, flex: 1 })}>
                <div style={sx({ display: 'flex', alignItems: 'center', gap: 7 })}>
                  <span style={sx({ fontWeight: 700, fontSize: 15, letterSpacing: '-0.02em' })}>WordForge</span>
                  <span class="mono" style={sx({ fontSize: 9.5, fontWeight: 600, color: 'var(--text-3)', border: '1px solid var(--border)', borderRadius: 5, padding: '1px 5px' })}>管理后台</span>
                </div>
                <div class="eyebrow" style={sx({ fontSize: 9.5, marginTop: 3, letterSpacing: '0.14em' })}>ADMIN CONSOLE</div>
              </div>
            </Show>
          </div>

          {/* search trigger → opens existing CommandPalette */}
          <div style={sx({ padding: '12px 12px 6px' })}>
            <button
              onClick={openPalette}
              style={sx({ width: '100%', display: 'flex', alignItems: 'center', gap: 8, padding: collapsed() ? '8px' : '8px 11px', borderRadius: 10, justifyContent: collapsed() ? 'center' : 'flex-start', cursor: 'pointer', fontSize: 12.5, color: 'var(--text-3)', border: '1px solid var(--border)', background: 'var(--surface-sunken)' })}
              title="快速跳转 ⌘K"
            >
              <Icon name="search" size={15} />
              <Show when={!collapsed()}>
                <span style={sx({ flex: 1, textAlign: 'left' })}>快速跳转…</span>
                <span class="kbd">⌘K</span>
              </Show>
            </button>
          </div>

          {/* nav */}
          <nav style={sx({ flex: 1, overflowY: 'auto', padding: '6px 10px 14px' })}>
            <For each={NAV}>
              {(g) => (
                <div style={sx({ marginTop: 10 })}>
                  <Show when={!collapsed()} fallback={<div style={sx({ height: 1, background: 'var(--hairline)', margin: '8px 6px' })} />}>
                    <div class="eyebrow" style={sx({ padding: '6px 8px 4px', fontSize: 10 })}>{g.group}</div>
                  </Show>
                  <For each={g.items}>
                    {(it) => (
                      <A
                        href={it.href}
                        end={it.exact}
                        onClick={() => setMobileOpen(false)}
                        title={collapsed() ? it.label : undefined}
                        style={sx({
                          width: '100%', display: 'flex', alignItems: 'center', gap: 11,
                          padding: collapsed() ? '9px' : '8px 10px', margin: '1px 0', borderRadius: 10,
                          cursor: 'pointer', justifyContent: collapsed() ? 'center' : 'flex-start',
                          background: isActive(it) ? 'var(--accent-soft)' : 'transparent',
                          color: isActive(it) ? 'var(--accent)' : 'var(--text-2)',
                          fontWeight: isActive(it) ? 600 : 500, fontSize: 13,
                          transition: 'background .15s, color .15s', position: 'relative',
                        })}
                      >
                        <Show when={isActive(it)}>
                          <span style={sx({ position: 'absolute', left: 0, top: 8, bottom: 8, width: 3, borderRadius: 3, background: 'var(--accent)' })} />
                        </Show>
                        <Icon name={it.icon} size={18} style={sx({ flex: 'none', color: isActive(it) ? 'var(--accent)' : 'var(--text-3)' })} />
                        <Show when={!collapsed()}>
                          <span style={sx({ flex: 1, textAlign: 'left', whiteSpace: 'nowrap' })}>{it.label}</span>
                          <Show when={it.badge}>
                            <span style={sx({ fontSize: 9, fontWeight: 700, padding: '2px 6px', borderRadius: 5, background: 'var(--accent-mid)', color: 'var(--accent)' })}>{it.badge}</span>
                          </Show>
                        </Show>
                      </A>
                    )}
                  </For>
                </div>
              )}
            </For>
          </nav>

          {/* collapse */}
          <div style={sx({ padding: 10, borderTop: '1px solid var(--hairline)' })}>
            <button
              onClick={() => setCollapsed((c) => !c)}
              class="btn btn-ghost"
              style={sx({ width: '100%', justifyContent: collapsed() ? 'center' : 'flex-start', gap: 9, fontSize: 12.5 })}
              title={collapsed() ? '展开' : '折叠'}
            >
              <Icon name="chevR" size={15} style={sx({ transform: collapsed() ? 'none' : 'rotate(180deg)', transition: 'transform .2s' })} />
              <Show when={!collapsed()}>折叠侧边栏</Show>
            </button>
          </div>
        </aside>

        {/* main */}
        <div style={sx({ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' })}>
          <header
            class="glass"
            style={sx({ height: 'var(--topbar-h)', flex: 'none', display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, padding: '0 22px', borderBottom: '1px solid var(--border)', position: 'relative', zIndex: 50 })}
          >
            <div style={sx({ display: 'flex', alignItems: 'center', gap: 10, minWidth: 0 })}>
              <button
                class="btn btn-ghost btn-icon wf-mobile-only"
                onClick={() => setMobileOpen(true)}
                title="菜单" aria-label="打开导航菜单"
              >
                <Icon name="grid" size={17} />
              </button>
              <h1 class="fade-in" style={sx({ margin: 0, fontSize: 16.5, fontWeight: 700, letterSpacing: '-0.01em' })}>{pageTitle()}</h1>
            </div>
            <div style={sx({ display: 'flex', alignItems: 'center', gap: 6 })}>
              <button class="btn btn-secondary btn-sm wf-desktop-only" onClick={openPalette} style={sx({ gap: 7 })}>
                <Icon name="search" size={14} />跳转<span class="kbd">⌘K</span>
              </button>
              <NotificationBell />
              <button
                class="btn btn-ghost btn-icon"
                title="新功能导览" aria-label="新功能导览"
                onClick={() => onboarding.show()}
              >
                <Icon name="info" size={17} />
              </button>
              <button
                class="btn btn-ghost btn-icon"
                title={themeStore.effective() === 'dark' ? '切到亮色' : '切到暗色'}
                onClick={() => themeStore.toggle()}
              >
                <Icon name={themeStore.effective() === 'dark' ? 'sun' : 'moon'} size={17} />
              </button>
              <div title="账户" style={sx({ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 9px 4px 5px', borderRadius: 10, marginLeft: 2 })}>
                <span style={sx({ width: 28, height: 28, borderRadius: 8, background: 'var(--accent)', color: '#fff', display: 'grid', placeItems: 'center', fontSize: 12, fontWeight: 600, flex: 'none' })}>
                  {(adminEmail() || 'A').slice(0, 1).toUpperCase()}
                </span>
                <span class="wf-desktop-only" style={sx({ fontSize: 12.5, fontWeight: 600, color: 'var(--text)', maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' })}>{adminEmail() || '管理员'}</span>
              </div>
              <button class="btn btn-ghost btn-icon" title="退出登录" aria-label="退出登录" onClick={doLogout}>
                <Icon name="logout" size={17} />
              </button>
            </div>
          </header>

          <ClockDriftWarning />

          <main style={sx({ flex: 1, overflowY: 'auto', padding: '24px 22px 60px' })}>
            <Show when={location.pathname} keyed>
              <div class="fade-up" style={sx({ maxWidth: 'var(--wf-content-max)', margin: '0 auto' })}>
                {props.children}
              </div>
            </Show>
          </main>
        </div>
      </div>

      {/* existing overlays — wired, mounted inside the .wf-admin scope */}
      <CommandPalette />
      <OnboardingTour />
    </div>
  );
}
