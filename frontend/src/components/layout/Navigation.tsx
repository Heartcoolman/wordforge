import { Show, createSignal, For } from 'solid-js';
import { A, useLocation } from '@solidjs/router';
import { cn } from '@/utils/cn';
import { authStore } from '@/stores/auth';
import { themeStore } from '@/stores/theme';

interface NavLink {
  href: string;
  label: string;
  icon: string;
  auth?: boolean;
}

const navLinks: NavLink[] = [
  { href: '/', label: '首页', icon: 'M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6' },
  { href: '/learning', label: '学习', icon: 'M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253', auth: true },
  { href: '/flashcard', label: '闪记', icon: 'M13 10V3L4 14h7v7l9-11h-7z', auth: true },
  { href: '/vocabulary', label: '词库', icon: 'M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10', auth: true },
  { href: '/wordbooks', label: '词书', icon: 'M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z', auth: true },
  { href: '/statistics', label: '统计', icon: 'M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z', auth: true },
];

function NavIcon(props: { d: string }) {
  return (
    <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d={props.d} />
    </svg>
  );
}

export function Navigation() {
  const location = useLocation();
  const [mobileOpen, setMobileOpen] = createSignal(false);
  const isActive = (href: string) =>
    href === '/' ? location.pathname === '/' : location.pathname.startsWith(href);

  const visibleLinks = () => navLinks.filter((l) => !l.auth || authStore.isAuthenticated());

  return (
    <nav class="sticky top-0 z-40 bg-surface/80 backdrop-blur-md border-b border-border">
      <div class="max-w-6xl mx-auto px-4 h-14 flex items-center justify-between">
        <A href="/" class="flex items-center gap-2 text-accent font-bold text-lg">
          <svg class="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
          </svg>
          WordMaster
        </A>

        <div class="hidden md:flex items-center gap-1">
          <For each={visibleLinks()}>
            {(link) => (
            <A
              href={link.href}
              class={cn(
                'flex items-center gap-1.5 px-3 py-2 rounded-lg text-sm font-medium transition-colors',
                isActive(link.href) ? 'bg-accent-light text-accent' : 'text-content-secondary hover:text-content hover:bg-surface-secondary',
              )}
            >
              <NavIcon d={link.icon} />
              {link.label}
            </A>
            )}
          </For>
        </div>

        <div class="flex items-center gap-2">
          <button onClick={() => themeStore.toggle()} class="p-2 rounded-lg text-content-secondary hover:text-content hover:bg-surface-secondary transition-colors cursor-pointer" aria-label={themeStore.effective() === 'dark' ? '切换为浅色模式' : '切换为深色模式'} title={`Theme: ${themeStore.mode()}`}>
            <Show when={themeStore.effective() === 'dark'} fallback={
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" /></svg>
            }>
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" /></svg>
            </Show>
          </button>

          <Show when={authStore.isAuthenticated()} fallback={
            <div class="flex items-center gap-2">
              <A href="/login" class="text-sm text-content-secondary hover:text-content">登录</A>
              <A href="/register" class="text-sm px-3 py-1.5 bg-accent text-accent-content rounded-lg hover:bg-accent-hover">注册</A>
            </div>
          }>
            <A href="/profile" class="p-2 rounded-lg text-content-secondary hover:text-content hover:bg-surface-secondary transition-colors">
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" /></svg>
            </A>
          </Show>

          <button
            class="md:hidden p-2 rounded-lg text-content-secondary hover:bg-surface-secondary cursor-pointer"
            onClick={() => setMobileOpen(!mobileOpen())}
            aria-label="菜单"
            aria-expanded={mobileOpen()}
          >
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d={mobileOpen() ? 'M6 18L18 6M6 6l12 12' : 'M4 6h16M4 12h16M4 18h16'} />
            </svg>
          </button>
        </div>
      </div>

      <Show when={mobileOpen()}>
        <div class="md:hidden border-t border-border bg-surface animate-fade-in px-4 py-2 space-y-1">
          <For each={visibleLinks()}>
            {(link) => (
            <A href={link.href} onClick={() => setMobileOpen(false)} class={cn(
              'flex items-center gap-2 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors',
              isActive(link.href) ? 'bg-accent-light text-accent' : 'text-content-secondary hover:bg-surface-secondary',
            )}>
              <NavIcon d={link.icon} />
              {link.label}
            </A>
            )}
          </For>
        </div>
      </Show>
    </nav>
  );
}
