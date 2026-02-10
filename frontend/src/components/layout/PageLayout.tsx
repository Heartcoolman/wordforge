import { type ParentProps, onMount, createEffect } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Navigation } from './Navigation';
import { authStore } from '@/stores/auth';
import { unauthorized } from '@/api/client';

export function PageLayout(props: ParentProps) {
  const navigate = useNavigate();

  onMount(() => {
    authStore.init();
  });

  // Watch for 401 and redirect via SPA router (no hard refresh)
  createEffect(() => {
    if (unauthorized()) {
      const path = window.location.pathname;
      if (!path.startsWith('/login') && !path.startsWith('/admin')) {
        navigate('/login', { replace: true });
      }
    }
  });

  return (
    <div class="min-h-screen flex flex-col">
      <Navigation />
      <main class="flex-1">
        <div class="max-w-6xl mx-auto px-4 py-6">
          {props.children}
        </div>
      </main>
      <footer class="border-t border-border py-4 text-center text-xs text-content-tertiary">
        WordMaster - 智能英语词汇学习平台
      </footer>
    </div>
  );
}
