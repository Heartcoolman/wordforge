import { A } from '@solidjs/router';
import { Show } from 'solid-js';

const USER_APP_URL = (import.meta.env.VITE_USER_APP_URL as string | undefined)?.trim();

function buildUserAppHref(pathname: string): string | null {
  if (!USER_APP_URL) return null;
  try {
    return new URL(pathname, USER_APP_URL).toString();
  } catch {
    return USER_APP_URL;
  }
}

export default function LegacyUserFrontendPage() {
  const pathname = window.location.pathname;
  const userAppHref = buildUserAppHref(pathname);
  const isHome = pathname === '/';

  return (
    <main class="min-h-screen bg-[radial-gradient(circle_at_top,_rgba(14,165,233,0.18),_transparent_38%),linear-gradient(180deg,_#f7fbff_0%,_#eef6ff_100%)] text-slate-900">
      <div class="mx-auto flex min-h-screen max-w-3xl items-center px-6 py-16">
        <section class="w-full rounded-3xl border border-sky-100 bg-white/90 p-8 shadow-[0_24px_80px_rgba(15,23,42,0.10)] backdrop-blur">
          <p class="text-sm font-semibold uppercase tracking-[0.28em] text-sky-600">WordForge</p>
          <h1 class="mt-4 text-3xl font-semibold tracking-tight text-slate-950">
            用户前端已迁移到独立仓库
          </h1>
          <p class="mt-4 text-base leading-7 text-slate-600">
            当前仓库继续提供管理后台前端和完整 API 服务。原来的学习端 Web 页面已经拆分到独立项目
            `wordforge-web`。
          </p>
          <p class="mt-3 text-sm leading-6 text-slate-500">
            {isHome ? '如果你是管理员，可以直接进入后台。' : `你访问的旧路径是 ${pathname}。`}
          </p>

          <div class="mt-8 flex flex-wrap gap-3">
            <Show
              when={userAppHref}
              fallback={
                <span class="inline-flex items-center rounded-full bg-slate-100 px-4 py-2 text-sm text-slate-600">
                  未配置 `VITE_USER_APP_URL`，请使用独立部署的用户前端地址
                </span>
              }
            >
              <a
                href={userAppHref!}
                class="inline-flex items-center rounded-full bg-sky-600 px-5 py-2.5 text-sm font-medium text-white transition hover:bg-sky-700"
              >
                前往用户前端
              </a>
            </Show>
            <A
              href="/admin"
              class="inline-flex items-center rounded-full border border-slate-200 px-5 py-2.5 text-sm font-medium text-slate-700 transition hover:border-slate-300 hover:bg-slate-50"
            >
              打开管理后台
            </A>
          </div>
        </section>
      </div>
    </main>
  );
}
