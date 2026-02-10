import { type ParentProps, Show, createSignal, For } from 'solid-js';
import { A, useLocation, useNavigate } from '@solidjs/router';
import { cn } from '@/utils/cn';
import { tokenManager } from '@/lib/token';

const sidebarLinks = [
  { href: '/admin', label: '仪表盘', icon: 'M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z', exact: true },
  { href: '/admin/users', label: '用户管理', icon: 'M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z' },
  { href: '/admin/amas-config', label: 'AMAS 配置', icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z' },
  { href: '/admin/monitoring', label: '系统监控', icon: 'M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z' },
  { href: '/admin/analytics', label: '数据分析', icon: 'M16 8v8m-4-5v5m-4-2v2m-2 4h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z' },
  { href: '/admin/settings', label: '系统设置', icon: 'M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4' },
];

export function AdminLayout(props: ParentProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [collapsed, setCollapsed] = createSignal(false);

  const isActive = (href: string, exact?: boolean) =>
    exact ? location.pathname === href : location.pathname.startsWith(href);

  return (
    <div class="min-h-screen flex bg-surface-secondary">
      {/* Sidebar */}
      <aside class={cn(
        'fixed left-0 top-0 h-screen bg-surface border-r border-border flex flex-col z-30 transition-all duration-200',
        collapsed() ? 'w-16' : 'w-56',
      )}>
        <div class="h-14 flex items-center justify-between px-4 border-b border-border">
          <Show when={!collapsed()}>
            <span class="font-bold text-accent">Admin</span>
          </Show>
          <button
            onClick={() => setCollapsed(!collapsed())}
            class="p-1.5 rounded-lg text-content-tertiary hover:text-content hover:bg-surface-secondary transition-colors cursor-pointer"
          >
            <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d={collapsed() ? 'M13 5l7 7-7 7' : 'M11 19l-7-7 7-7'} />
            </svg>
          </button>
        </div>

        <nav class="flex-1 py-3 px-2 space-y-1 overflow-y-auto">
          <For each={sidebarLinks}>
            {(link) => (
            <A
              href={link.href}
              class={cn(
                'flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors',
                isActive(link.href, link.exact)
                  ? 'bg-accent-light text-accent'
                  : 'text-content-secondary hover:text-content hover:bg-surface-secondary',
                collapsed() && 'justify-center px-2',
              )}
              title={collapsed() ? link.label : undefined}
            >
              <svg class="w-5 h-5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d={link.icon} />
              </svg>
              <Show when={!collapsed()}>
                <span>{link.label}</span>
              </Show>
            </A>
            )}
          </For>
        </nav>

        <div class="border-t border-border p-3">
          <button
            onClick={() => {
              tokenManager.clearAdminToken();
              navigate('/admin/login', { replace: true });
            }}
            class={cn(
              'flex items-center gap-2 w-full px-3 py-2 rounded-lg text-sm text-content-secondary hover:text-error hover:bg-error-light transition-colors cursor-pointer',
              collapsed() && 'justify-center px-2',
            )}
          >
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
            </svg>
            <Show when={!collapsed()}>退出</Show>
          </button>
        </div>
      </aside>

      {/* Main */}
      <div class={cn('flex-1 transition-all duration-200', collapsed() ? 'ml-16' : 'ml-56')}>
        <header class="h-14 bg-surface border-b border-border flex items-center px-6">
          <h1 class="text-lg font-semibold text-content">管理后台</h1>
        </header>
        <main class="p-6">{props.children}</main>
      </div>
    </div>
  );
}
