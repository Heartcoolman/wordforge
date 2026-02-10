import { type ParentProps, Show, createSignal, onMount, onCleanup } from 'solid-js';
import { Navigate } from '@solidjs/router';
import { authStore } from '@/stores/auth';
import { tokenManager } from '@/lib/token';
import { Spinner } from '@/components/ui/Spinner';

export function ProtectedRoute(props: ParentProps) {
  return (
    <Show
      when={!authStore.loading()}
      fallback={
        <div class="flex items-center justify-center min-h-[60vh]">
          <Spinner size="lg" />
        </div>
      }
    >
      <Show when={authStore.isAuthenticated()} fallback={<Navigate href="/login" />}>
        {props.children}
      </Show>
    </Show>
  );
}

export function AdminProtectedRoute(props: ParentProps) {
  const [hasToken, setHasToken] = createSignal<boolean | null>(null);

  const recheck = () => setHasToken(tokenManager.getAdminToken() !== null);

  onMount(() => {
    recheck();
    window.addEventListener('focus', recheck);
  });

  onCleanup(() => {
    window.removeEventListener('focus', recheck);
  });

  return (
    <Show
      when={hasToken() !== null}
      fallback={
        <div class="flex items-center justify-center min-h-[60vh]">
          <Spinner size="lg" />
        </div>
      }
    >
      <Show when={hasToken()} fallback={<Navigate href="/admin/login" />}>
        {props.children}
      </Show>
    </Show>
  );
}
