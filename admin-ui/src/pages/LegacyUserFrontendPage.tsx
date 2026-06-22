import { A, useLocation } from '@solidjs/router';
import { Show, createMemo } from 'solid-js';

const USER_APP_URL = (import.meta.env.VITE_USER_APP_URL as string | undefined)?.trim();

function buildUserAppHref(targetPath: string): string | null {
  if (!USER_APP_URL) return null;
  try {
    return new URL(targetPath, USER_APP_URL).toString();
  } catch {
    return USER_APP_URL;
  }
}

export default function LegacyUserFrontendPage() {
  const location = useLocation();
  // useLocation 让 currentPath 在 SPA 内部路由变化时跟随，而不是固定到首次挂载的 window.location
  const currentPath = createMemo(() => `${location.pathname}${location.search}${location.hash}`);
  const userAppHref = createMemo(() => buildUserAppHref(currentPath()));
  const isHome = createMemo(() => location.pathname === '/');

  return (
    <main class="min-h-screen bg-surface-secondary text-content">
      <div class="mx-auto flex min-h-screen max-w-3xl items-center px-6 py-16">
        <section class="w-full rounded-3xl border border-border bg-surface p-8 shadow-[var(--shadow)]">
          <p class="font-mono text-[11px] font-medium uppercase tracking-[0.18em] text-content-tertiary">WordForge</p>
          <h1 class="mt-4 text-3xl font-bold tracking-[-0.03em] text-content">
            用户前端已迁移到独立仓库
          </h1>
          <p class="mt-4 text-base leading-7 text-content-secondary">
            当前仓库继续提供管理后台前端和完整 API 服务。原来的学习端 Web 页面已经拆分到独立项目
            {' '}
            <code class="rounded-md bg-surface-secondary px-1.5 py-0.5 font-mono text-[0.9em] text-content-secondary">wordforge-web</code>
            。
          </p>
          <p class="mt-3 text-sm leading-6 text-content-tertiary">
            {isHome() ? '如果你是管理员，可以直接进入后台。' : `你访问的旧路径是 ${currentPath()}。`}
          </p>

          <div class="mt-8 flex flex-wrap gap-3">
            <Show
              when={userAppHref()}
              fallback={
                <span class="inline-flex items-center rounded-full bg-warning-light px-4 py-2 text-sm text-warning-strong">
                  用户前端尚未上线，请联系管理员
                </span>
              }
            >
              <a
                href={userAppHref()!}
                class="inline-flex items-center rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-[var(--solid-ink)] transition hover:opacity-90"
              >
                前往用户前端
              </a>
            </Show>
            <A
              href="/admin"
              class="inline-flex items-center rounded-full border border-border px-5 py-2.5 text-sm font-medium text-content transition hover:border-border-strong hover:bg-surface-secondary"
            >
              打开管理后台
            </A>
          </div>
        </section>
      </div>
    </main>
  );
}
