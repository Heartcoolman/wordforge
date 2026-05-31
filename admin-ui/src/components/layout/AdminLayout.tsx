import { type ParentProps, Show, createSignal, For, onMount } from 'solid-js';
import { A, useLocation, useNavigate } from '@solidjs/router';
import { cn } from '@/utils/cn';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';
import { themeStore } from '@/stores/theme';
import { Skeleton } from '@/components/ui/Skeleton';
import { Kbd } from '@/components/ui/Kbd';
import { CommandPalette } from '@/components/ui/CommandPalette';
import { OnboardingTour } from '@/components/onboarding/OnboardingTour';
import { useOnboarding } from '@/components/onboarding/useOnboarding';
import { useIndicatorTrack } from '@/lib/motion';
import { ClockDriftWarning } from '@/components/admin/ClockDriftWarning';

// Sidebar 宽度对齐 brand-spec：240 expanded / 64 collapsed
const SIDEBAR_W_OPEN = 'md:w-60'; // 240px
const SIDEBAR_W_COLLAPSED = 'md:w-16'; // 64px
const MAIN_ML_OPEN = 'md:ml-60';
const MAIN_ML_COLLAPSED = 'md:ml-16';

type SidebarItem =
  | { kind: 'section'; label: string }
  | { kind: 'link'; href: string; label: string; icon: string; exact?: boolean; isNew?: boolean };

/**
 * 4 section 分组导航。section label 在折叠态隐藏。
 * 新增 broadcast / resource-packs 标 isNew，在 sidebar 显示小 chip。
 */
const sidebarItems: SidebarItem[] = [
  { kind: 'section', label: '主区' },
  { kind: 'link', href: '/admin', label: '仪表盘', exact: true,
    icon: 'M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z' },
  { kind: 'link', href: '/admin/users', label: '用户管理',
    icon: 'M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z' },
  { kind: 'link', href: '/admin/clients', label: '设备管理',
    icon: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z' },

  { kind: 'section', label: '学习引擎' },
  { kind: 'link', href: '/admin/amas-config', label: 'AMAS 调参',
    icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z' },
  { kind: 'link', href: '/admin/amas-metrics', label: 'AMAS 指标',
    icon: 'M3 3v18h18M7 14l4-4 4 4 5-5' },
  { kind: 'link', href: '/admin/amas-advisor', label: 'LLM 顾问',
    icon: 'M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z' },

  { kind: 'section', label: '运维' },
  { kind: 'link', href: '/admin/monitoring', label: '系统监控',
    icon: 'M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z' },
  { kind: 'link', href: '/admin/analytics', label: '数据分析',
    icon: 'M16 8v8m-4-5v5m-4-2v2m-2 4h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z' },
  { kind: 'link', href: '/admin/wordbook-center', label: '词库中心',
    icon: 'M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253' },
  { kind: 'link', href: '/admin/feedback', label: '用户反馈',
    icon: 'M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z' },
  { kind: 'link', href: '/admin/probe', label: '数据探针', isNew: true,
    icon: 'M12 21a9 9 0 100-18 9 9 0 000 18zm0-4a5 5 0 100-10 5 5 0 000 10zm0-3a2 2 0 100-4 2 2 0 000 4z' },
  { kind: 'link', href: '/admin/remote-probe', label: '远程探针',
    icon: 'M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25' },

  { kind: 'section', label: '系统' },
  { kind: 'link', href: '/admin/updates', label: '版本更新',
    icon: 'M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15' },
  { kind: 'link', href: '/admin/resource-packs', label: '资源包', isNew: true,
    icon: 'M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16zM3.27 6.96L12 12.01l8.73-5.05M12 22.08V12' },
  { kind: 'link', href: '/admin/broadcast', label: '系统广播', isNew: true,
    icon: 'M11 5.882V19.24a1.76 1.76 0 01-3.417.592l-2.147-6.15M18 13a3 3 0 100-6M5.436 13.683A4.001 4.001 0 017 6h1.832c4.1 0 7.625-1.234 9.168-3v14c-1.543-1.766-5.067-3-9.168-3H7a3.988 3.988 0 01-1.564-.317z' },
  { kind: 'link', href: '/admin/settings', label: '系统设置',
    icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065zM15 12a3 3 0 11-6 0 3 3 0 016 0z' },
];

const linkItems = sidebarItems.filter((i): i is Extract<SidebarItem, { kind: 'link' }> => i.kind === 'link');

export function AdminLayout(props: ParentProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = createSignal(false);
  const [mobileOpen, setMobileOpen] = createSignal(false);
  const [adminEmail, setAdminEmail] = createSignal<string | null>(null);
  const [emailLoading, setEmailLoading] = createSignal(true);
  const onboarding = useOnboarding();
  let navRef: HTMLElement | undefined;

  const isActive = (href: string, exact?: boolean) =>
    exact ? location.pathname === href : location.pathname.startsWith(href);

  const activeHref = () => {
    const match = linkItems.find((link) => isActive(link.href, link.exact));
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
    const match = linkItems.find((link) =>
      link.exact ? path === link.href : path.startsWith(link.href),
    );
    return match?.label ?? '管理后台';
  };

  onMount(async () => {
    try {
      const res = await adminApi.verifyToken();
      setAdminEmail(res.email);
    } catch { /* silent */ }
    finally { setEmailLoading(false); }
    // 这一大版本波次(1.1.x~1.2.x)首次进入自动弹一次新功能导览；
    // 同波重复升级不重弹，跨大版本(1.3+)再弹。版本取自公开 /health。
    try {
      const h = await adminApi.health();
      onboarding.autoShowIfNeeded(h.version);
    } catch { /* 拿不到版本则不弹 */ }
  });

  return (
    <div class="min-h-screen flex bg-surface-secondary">
      <Show when={mobileOpen()}>
        <div
          role="presentation"
          aria-hidden="true"
          class="fixed inset-0 z-20 bg-black/40 backdrop-blur-sm md:hidden animate-fade-in"
          onClick={() => setMobileOpen(false)}
        />
      </Show>

      {/* Sidebar — overflow-hidden 兜底，避免折叠态 icon hover scale 溢出到容器外 */}
      <aside class={cn(
        'fixed left-0 top-0 h-screen bg-surface-elevated border-r border-border-hairline flex flex-col z-30 overflow-hidden',
        'transition-[width,transform] duration-base ease-out-expo',
        'w-72',
        collapsed() ? SIDEBAR_W_COLLAPSED : SIDEBAR_W_OPEN,
        mobileOpen() ? 'translate-x-0' : '-translate-x-full md:translate-x-0',
      )}>
        {/* Brand row */}
        <div class="flex items-center gap-2.5 px-4 pt-[18px] pb-3.5 border-b border-border-hairline whitespace-nowrap overflow-hidden">
          <div class="size-8 rounded-[9px] shrink-0 shadow-elevation-1 bg-gradient-brand-mark relative grid place-items-center text-white font-bold text-[16px] leading-none tracking-[-0.04em]">
            W
          </div>
          <Show when={!collapsed() || mobileOpen()}>
            <div class="flex flex-col leading-[1.15] font-sans min-w-0 flex-1">
              <strong class="font-bold text-[14px] tracking-[-0.012em] truncate">WordForge Admin</strong>
              <span class="text-[11px] text-content-tertiary">运维 GUI</span>
            </div>
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
            class="p-1 rounded text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer shrink-0"
          >
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d={mobileOpen() ? 'M6 18L18 6M6 6l12 12' : collapsed() ? 'M13 5l7 7-7 7' : 'M11 19l-7-7 7-7'} />
            </svg>
          </button>
        </div>

        {/* Search button → triggers CommandPalette via [data-palette-open] */}
        <div class="px-3 py-3 border-b border-border-hairline">
          <button
            data-palette-open
            type="button"
            class={cn(
              'w-full inline-flex items-center gap-2 px-2.5 py-1.5',
              'bg-surface-secondary border border-border-hairline rounded-md',
              'text-content-tertiary text-[12.5px] hover:bg-surface-tertiary hover:border-border',
              'transition-colors duration-fast cursor-pointer',
              collapsed() && !mobileOpen() && 'md:justify-center md:px-1.5',
            )}
            aria-label="打开命令面板"
            title="跳转到任意页面"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="shrink-0">
              <circle cx="11" cy="11" r="7" />
              <path d="m21 21-4.3-4.3" />
            </svg>
            <Show when={!collapsed() || mobileOpen()}>
              <span class="flex-1 text-left">快速跳转…</span>
              <Kbd size="sm">⌘K</Kbd>
            </Show>
          </button>
        </div>

        {/* Nav with indicator */}
        <nav ref={navRef} class="relative flex-1 py-2 px-2 overflow-y-auto">
          <div
            aria-hidden="true"
            class="absolute left-0 top-0 rounded-md bg-accent-light pointer-events-none transition-[transform,width,height,opacity] duration-base ease-out-expo"
            style={{
              transform: `translate(${indicator().left}px, ${indicator().top}px)`,
              width: `${indicator().width}px`,
              height: `${indicator().height}px`,
              opacity: indicator().height > 0 ? 1 : 0,
            }}
          />
          <For each={sidebarItems}>{(item) => (
            <Show
              when={item.kind === 'link'}
              fallback={
                <Show when={!collapsed() || mobileOpen()}>
                  <div class="px-2.5 pt-3 pb-1 text-[10.5px] font-semibold tracking-[0.08em] uppercase text-content-tertiary">
                    {(item as Extract<SidebarItem, { kind: 'section' }>).label}
                  </div>
                </Show>
              }
            >
              {(() => {
                const link = item as Extract<SidebarItem, { kind: 'link' }>;
                return (
                  <A
                    href={link.href}
                    data-sidebar-active={isActive(link.href, link.exact) ? 'true' : undefined}
                    aria-current={isActive(link.href, link.exact) ? 'page' : undefined}
                    class={cn(
                      'group relative flex items-center gap-3 px-2.5 py-1.5 my-px rounded-md text-[13px] font-medium',
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
                      class={cn(
                        'w-[18px] h-[18px] flex-shrink-0 transition-transform duration-fast ease-out-expo group-hover:scale-110',
                        isActive(link.href, link.exact) ? 'text-accent' : 'text-content-tertiary',
                      )}
                      fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"
                    >
                      <path stroke-linecap="round" stroke-linejoin="round" d={link.icon} />
                    </svg>
                    <Show when={!collapsed() || mobileOpen()}>
                      <span class="truncate">{link.label}</span>
                      <Show when={link.isNew}>
                        <span class="ml-auto text-[10px] font-semibold px-1.5 py-px rounded bg-accent-light text-accent">NEW</span>
                      </Show>
                    </Show>
                  </A>
                );
              })()}
            </Show>
          )}</For>
        </nav>
      </aside>

      {/* Main */}
      <div class={cn('flex-1 min-w-0 transition-[margin] duration-base ease-out-expo', collapsed() ? MAIN_ML_COLLAPSED : MAIN_ML_OPEN)}>
        <header class="sticky top-0 z-10 h-16 bg-surface/80 backdrop-blur-md border-b border-border-hairline flex items-center justify-between gap-3 px-4 sm:px-6">
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
            <Show when={pageTitle()} keyed>
              {(title) => <h1 class="truncate text-base sm:text-lg font-semibold text-content animate-fade-in">{title}</h1>}
            </Show>
          </div>
          <div class="flex items-center gap-1.5 sm:gap-2 min-w-0">
            {/* ⌘K 命令面板入口（mobile 隐藏，sidebar 已有 search 按钮） */}
            <button
              data-palette-open
              type="button"
              class="hidden md:inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-md bg-surface-secondary text-content-tertiary text-xs hover:bg-surface-tertiary hover:text-content transition-colors duration-fast cursor-pointer focus-ring-soft"
              aria-label="打开命令面板"
              title="⌘K 跳转任意页面"
            >
              <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <circle cx="11" cy="11" r="7" />
                <path d="m21 21-4.3-4.3" />
              </svg>
              <span>跳转</span>
              <Kbd size="sm">⌘K</Kbd>
            </button>

            {/* 新功能导览入口（随时重看） */}
            <button
              type="button"
              onClick={() => onboarding.show()}
              class="p-1.5 rounded-md text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer focus-ring-soft"
              aria-label="新功能导览"
              title="新功能导览"
            >
              <svg class="w-[18px] h-[18px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 22a10 10 0 100-20 10 10 0 000 20z" />
                <path stroke-linecap="round" stroke-linejoin="round" d="M9.1 9a3 3 0 015.8 1c0 2-3 3-3 3" />
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 17h.01" />
              </svg>
            </button>

            {/* 主题切换 */}
            <button
              type="button"
              onClick={() => themeStore.toggle()}
              class="p-1.5 rounded-md text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors duration-fast cursor-pointer focus-ring-soft"
              aria-label={`切换主题（当前 ${themeStore.mode()}）`}
              title={`主题：${themeStore.mode() === 'light' ? '亮色' : themeStore.mode() === 'dark' ? '暗色' : '跟随系统'}`}
            >
              <Show
                when={themeStore.effective() === 'dark'}
                fallback={
                  <svg class="w-[18px] h-[18px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                    <circle cx="12" cy="12" r="4" />
                    <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
                  </svg>
                }
              >
                <svg class="w-[18px] h-[18px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                  <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
                </svg>
              </Show>
            </button>

            {/* admin chip */}
            <Show when={!emailLoading()} fallback={<Skeleton width="100px" />}>
              <Show when={adminEmail()}>
                <span class="hidden sm:inline-flex items-center gap-1.5 px-2 py-1 rounded-full bg-accent-light/60 text-accent text-xs font-medium max-w-[180px]">
                  <span class="size-1.5 rounded-full bg-accent" />
                  <span class="truncate">{adminEmail()}</span>
                </span>
              </Show>
            </Show>

            {/* 退出 */}
            <button
              onClick={async () => {
                try { await adminApi.logout(); } catch { uiStore.toast.warning('退出请求失败，已清理本地登录状态'); }
                finally { tokenManager.clearAdminToken(); navigate('/admin/login', { replace: true }); }
              }}
              class="p-1.5 rounded-md text-content-tertiary hover:text-error hover:bg-error-light transition-colors duration-fast cursor-pointer focus-ring-soft"
              title="退出登录"
              aria-label="退出登录"
            >
              <svg class="w-[18px] h-[18px]" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
              </svg>
            </button>
          </div>
        </header>
        <ClockDriftWarning />
        <main class="p-4 sm:p-6 max-w-[var(--content-max)] mx-auto w-full">
          <Show when={location.pathname} keyed>
            <div class="animate-fade-in-up">{props.children}</div>
          </Show>
        </main>
      </div>

      {/* 全局命令面板 —— 仅 admin 区域内挂载；监听全局 ⌘K，外部点击 [data-palette-open] 也触发 */}
      <CommandPalette />

      {/* 新功能导览 —— 全屏 overlay，受 useOnboarding 单例 signal 控制 */}
      <OnboardingTour />
    </div>
  );
}
