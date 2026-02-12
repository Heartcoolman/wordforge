import { type ParentProps, Show, createSignal, onMount, onCleanup } from 'solid-js';
import { Navigate, useNavigate } from '@solidjs/router';
import { authStore } from '@/stores/auth';
import { tokenManager } from '@/lib/token';
import { adminApi } from '@/api/admin';
import { Spinner } from '@/components/ui/Spinner';

const VALIDATION_THROTTLE_MS = 30_000;

export function ProtectedRoute(props: ParentProps) {
  const navigate = useNavigate();
  const [verified, setVerified] = createSignal(false);
  const [checking, setChecking] = createSignal(true);
  let lastValidated = 0;

  onMount(async () => {
    if (!authStore.initialized) {
      await authStore.init();
    }

    if (!authStore.isAuthenticated()) {
      setChecking(false);
      return;
    }

    try {
      const { usersApi } = await import('@/api/users');
      await usersApi.getMe();
      setVerified(true);
      lastValidated = Date.now();
    } catch {
      tokenManager.clearTokens();
      navigate('/login', { replace: true });
    } finally {
      setChecking(false);
    }
  });

  return (
    <Show
      when={!authStore.loading() && !checking()}
      fallback={
        <div class="flex items-center justify-center min-h-[60vh]">
          <Spinner size="lg" />
        </div>
      }
    >
      <Show when={verified()} fallback={<Navigate href="/login" />}>
        {props.children}
      </Show>
    </Show>
  );
}

export function AdminProtectedRoute(props: ParentProps) {
  const navigate = useNavigate();
  const [verified, setVerified] = createSignal(false);
  const [loading, setLoading] = createSignal(true);
  let lastValidated = 0;

  async function verifyAdmin() {
    const token = tokenManager.getAdminToken();
    if (!token) {
      navigate('/admin/login', { replace: true });
      setLoading(false);
      return;
    }
    try {
      await adminApi.verifyToken();
      setVerified(true);
      lastValidated = Date.now();
    } catch {
      tokenManager.clearAdminToken();
      navigate('/admin/login', { replace: true });
    } finally {
      setLoading(false);
    }
  }

  const handleFocus = () => {
    if (verified() && Date.now() - lastValidated > VALIDATION_THROTTLE_MS) {
      lastValidated = Date.now();
      adminApi.verifyToken().catch(() => {
        tokenManager.clearAdminToken();
        setVerified(false);
        navigate('/admin/login', { replace: true });
      });
    }
  };

  const handleUnauthorized = () => {
    setVerified(false);
    navigate('/admin/login', { replace: true });
  };

  onMount(() => {
    verifyAdmin();
    window.addEventListener('focus', handleFocus);
    window.addEventListener('admin:unauthorized', handleUnauthorized);
  });

  onCleanup(() => {
    window.removeEventListener('focus', handleFocus);
    window.removeEventListener('admin:unauthorized', handleUnauthorized);
  });

  return (
    <Show
      when={!loading()}
      fallback={
        <div class="flex items-center justify-center min-h-[60vh]">
          <Spinner size="lg" />
        </div>
      }
    >
      <Show when={verified()} fallback={<Navigate href="/admin/login" />}>
        {props.children}
      </Show>
    </Show>
  );
}
