import { type ParentProps, Show, createSignal, For, onMount } from 'solid-js';
import { A, useLocation, useNavigate } from '@solidjs/router';
import { cn } from '@/utils/cn';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';
import { Skeleton } from '@/components/ui/Skeleton';
import { useIndicatorTrack } from '@/lib/motion';

const sidebarLinks = [
  { href: '/admin', label: '仪表盘', icon: 'M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z', exact: true },
  { href: '/admin/users', label: '用户管理', icon: 'M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z' },
  { href: '/admin/clients', label: '客户端管理', icon: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z' },
  { href: '/admin/amas-config', label: 'AMAS 配置', icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z' },
  { href: '/admin/amas-metrics', label: 'AMAS 指标', icon: 'M3 3v18h18M7 14l4-4 4 4 5-5' },
  { href: '/admin/amas-advisor', label: 'AMAS 助手', icon: 'M12 2l3 7h7l-5.5 4.5L18 21l-6-4-6 4 1.5-7.5L2 9h7z' },
  { href: '/admin/monitoring', label: '系统监控', icon: 'M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z' },
  { href: '/admin/analytics', label: '数据分析', icon: 'M16 8v8m-4-5v5m-4-2v2m-2 4h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z' },
  { href: '/admin/wordbook-center', label: '词书中心', icon: 'M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253' },
  { href: '/admin/feedback', label: '用户反馈', icon: 'M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z' },
  { href: '/admin/updates', label: '版本更新', icon: 'M12 4v16m8-8H4' },
  { href: '/admin/settings', label: '系统设置', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
];

export function AdminLayout(props: ParentProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = createSignal(false);
  const [mobileOpen, setMobileOpen] = createSignal(false);
  const [adminEmail, setAdminEmail] = createSignal<string | null>(null);
  const [emailLoading, setEmailLoading] = createSignal(true);
  let navRef: HTMLElement | undefined;

  const isActive = (href: string, exact?: boolean) =>
    exact ? location.pathname === href : location.pathname.startsWith(href);

  // 算出 sidebar 当前激活项的 key（便于 indicator 跟踪）
  const activeHref = () => {
    const match = sidebarLinks.find((link) => isActive(link.href, link.exact));
    return match?.href ?? '';
  };

  // 侧边栏激活 indicator — 在选项之间平滑滑动
  const indicator = useIndicatorTrack(
    () => navRef,
    () => activeHref(),
    '[data-sidebar-active="true"]',
  );

  const pageTitle = () => {
    const path = location.pathname;
    const match = sidebarLinks.find(link =>
      link.exact ? path === link.href : path.startsWith(link.href)
    );
    return match?.label ?? '管理后台';
  };

  onMount(async () => {
    try {
      const res = await adminApi.verifyToken();
      setAdminEmail(res.email);
    } catch { /* silent */ }
    finally { setEmailLoading(false); }
  });

  return (
    <div class="min-h-screen flex bg-surface-secondary">
      <Show when={mobileOpen()}>
        <button
          type="button"
          aria-label="关闭导航菜单"
          class="fixed inset-0 z-20 bg-black/40 backdrop-blur-sm md:hidden animate-fade-in"
          onClick={() => setMobileOpen(false)}
        />
      </Show>

      {/* Sidebar */}
      <aside class={cn(
        'fixed left-0 top-0 h-screen bg-surface border-r border-border-hairline flex flex-col z-30',
        'transition-[width,transform] duration-base ease-out-expo',
        'w-72 md:w-56',
        collapsed() && 'md:w-16',
        mobileOpen() ? 'translate-x-0' : '-translate-x-full md:translate-x-0',
      )}>
        <div class="h-14 flex items-center justify-between px-4 border-b border-border-hairline">
          <Show when={!collapsed() || mobileOpen()}>
            <span class="font-bold text-accent">管理后台</span>
          </Show>
          <button
            onClick={() => {
              if (window.matchMedia('(min-width: 768px)').matches) {
                setCollapsed(!collapsed());
              } else {
                setMobileOpen(false);
              }
            }}
            aria-label={collapsed() ? '展开侧边栏' : '折叠侧边栏'}
            class="p-1.5 rounded-lg text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer"
          >
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d={mobileOpen() ? 'M6 18L18 6M6 6l12 12' : collapsed() ? 'M13 5l7 7-7 7' : 'M11 19l-7-7 7-7'} />
            </svg>
          </button>
        </div>

        <nav ref={navRef} class="relative flex-1 py-3 px-2 space-y-1 overflow-y-auto">
          {/* Indicator — 在激活项位置滑动 */}
          <div
            aria-hidden="true"
            class="absolute left-2 right-2 h-9 rounded-lg bg-accent-light pointer-events-none transition-[transform,opacity] duration-base ease-out-expo"
            style={{
              transform: `translateY(${indicator().top}px)`,
              opacity: indicator().height > 0 ? 1 : 0,
            }}
          />
          <For each={sidebarLinks}>
            {(link) => (
            <A
              href={link.href}
              data-sidebar-active={isActive(link.href, link.exact) ? 'true' : undefined}
              class={cn(
                'group relative flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium',
                'transition-colors duration-fast ease-out-expo',
                isActive(link.href, link.exact)
                  ? 'text-accent'
                  : 'text-content-secondary hover:text-content hover:bg-surface-secondary/60',
                collapsed() && 'md:justify-center md:px-2',
              )}
              title={collapsed() && !mobileOpen() ? link.label : undefined}
              onClick={() => setMobileOpen(false)}
            >
              <svg
                class="w-5 h-5 flex-shrink-0 transition-transform duration-fast ease-out-expo group-hover:scale-110"
                fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"
              >
                <path stroke-linecap="round" stroke-linejoin="round" d={link.icon} />
              </svg>
              <Show when={!collapsed() || mobileOpen()}>
                <span>{link.label}</span>
              </Show>
            </A>
            )}
          </For>
        </nav>
      </aside>

      {/* Main */}
      <div class={cn('flex-1 min-w-0 transition-[margin] duration-base ease-out-expo', collapsed() ? 'md:ml-16' : 'md:ml-56')}>
        <header class="sticky top-0 z-10 h-14 bg-surface/80 backdrop-blur-md border-b border-border-hairline flex items-center justify-between gap-3 px-4 sm:px-6">
          <div class="flex items-center gap-3 min-w-0">
            <button
              type="button"
              onClick={() => setMobileOpen(true)}
              aria-label="打开导航菜单"
              class="p-1.5 rounded-lg text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer md:hidden"
            >
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M4 6h16M4 12h16M4 18h16" />
              </svg>
            </button>
            {/* 标题切换 fade — Show keyed 在 pageTitle 变化时强制 remount，触发 CSS 入场 */}
            <Show when={pageTitle()} keyed>
              {(title) => <h1 class="truncate text-base sm:text-lg font-semibold text-content animate-fade-in">{title}</h1>}
            </Show>
          </div>
          <div class="flex items-center gap-2 sm:gap-3 min-w-0">
            <Show when={!emailLoading()} fallback={<Skeleton width="120px" />}>
              <Show when={adminEmail()}>
                <span class="hidden sm:inline truncate text-sm text-content-secondary">{adminEmail()}</span>
              </Show>
            </Show>
            <button
              onClick={async () => {
                try { await adminApi.logout(); } catch { uiStore.toast.warning('退出请求失败，已清理本地登录状态'); }
                finally { tokenManager.clearAdminToken(); navigate('/admin/login', { replace: true }); }
              }}
              class="p-1.5 rounded-lg text-content-tertiary hover:text-error hover:bg-error-light transition-colors duration-fast cursor-pointer"
              title="退出登录"
            >
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
              </svg>
            </button>
          </div>
        </header>
        <main class="p-4 sm:p-6">
          {/* 路由 children fade-in — Show keyed 在 pathname 变化时重新 mount 触发 CSS 入场 */}
          <Show when={location.pathname} keyed>
            <div class="animate-fade-in-up">{props.children}</div>
          </Show>
        </main>
      </div>
    </div>
  );
}
