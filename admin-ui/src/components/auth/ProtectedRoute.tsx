import { type ParentProps, Show, createSignal, onMount, onCleanup } from 'solid-js';
import { Navigate } from '@solidjs/router';
import { tokenManager } from '@/lib/token';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { Spinner } from '@/components/ui/Spinner';

// 30 秒节流，与 token 实际过期解耦；如需全局调整可迁到 @/lib/constants 集中管理
const VALIDATION_THROTTLE_MS = 30_000;

export function AdminProtectedRoute(props: ParentProps) {
  const [verified, setVerified] = createSignal(false);
  const [loading, setLoading] = createSignal(true);
  let lastValidated = 0;

  async function verifyAdmin() {
    const token = tokenManager.getAdminToken();
    if (!token) {
      // 无 token → 走 fallback <Navigate> 统一跳转，避免与 onMount navigate 双重重定向
      setLoading(false);
      return;
    }
    try {
      await adminApi.verifyToken();
      setVerified(true);
      lastValidated = Date.now();
    } catch {
      tokenManager.clearAdminToken();
      // 失败时让 fallback <Navigate> 接管跳转
    } finally {
      setLoading(false);
    }
  }

  const handleFocus = () => {
    if (verified() && Date.now() - lastValidated > VALIDATION_THROTTLE_MS) {
      lastValidated = Date.now();
      adminApi.verifyToken().catch(() => {
        tokenManager.clearAdminToken();
        uiStore.toast.warning('登录已过期，请重新登录');
        setVerified(false);
      });
    }
  };

  const handleUnauthorized = () => {
    setVerified(false);
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
