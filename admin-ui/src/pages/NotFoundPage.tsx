import { A, useLocation } from '@solidjs/router';
import { onMount, onCleanup, createMemo, Show } from 'solid-js';
import { Button } from '@/components/ui/Button';

export default function NotFoundPage() {
  const location = useLocation();
  // 管理员误入 /admin/* 404 时同时提供"返回 admin"出口，避免被踢到 marketing 首页丢上下文
  const isAdminPath = createMemo(() => location.pathname.startsWith('/admin'));

  onMount(() => {
    const prev = document.title;
    document.title = '404 · WordForge';
    onCleanup(() => {
      document.title = prev || 'WordForge';
    });
  });

  return (
    <div
      class="min-h-[60vh] flex flex-col items-center justify-center text-center animate-fade-in-up"
      role="region"
      aria-labelledby="notfound-title"
    >
      <p class="text-7xl font-bold text-accent mb-2" aria-hidden="true">404</p>
      <h1 id="notfound-title" class="text-2xl font-bold text-content mb-2">页面不存在</h1>
      <p class="text-content-secondary mb-6">你访问的页面可能已被移除或地址有误</p>
      <div class="flex flex-wrap items-center justify-center gap-3">
        <Show when={isAdminPath()}>
          <A href="/admin"><Button variant="outline">返回 admin</Button></A>
        </Show>
        <A href="/"><Button>返回首页</Button></A>
      </div>
    </div>
  );
}
