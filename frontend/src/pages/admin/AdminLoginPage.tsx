import { createSignal, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';
import { ADMIN_MAX_LOCK_WAIT_SECS } from '@/lib/constants';

export default function AdminLoginPage() {
  const navigate = useNavigate();
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');
  const [failCount, setFailCount] = createSignal(0);
  const [lockUntil, setLockUntil] = createSignal(0);

  onMount(async () => {
    const existingToken = tokenManager.getAdminToken();
    if (existingToken) {
      try {
        await adminApi.getHealth();
        navigate('/admin', { replace: true });
        return;
      } catch {
        tokenManager.clearAdminToken();
      }
    }

    // 再检查是否已初始化（需要网络请求）
    try {
      const status = await adminApi.checkStatus();
      if (!status.initialized) {
        navigate('/admin/setup', { replace: true });
      }
    } catch { /* ignore */ }
  });

  function getRemainingLockSeconds(): number {
    const remaining = Math.ceil((lockUntil() - Date.now()) / 1000);
    return remaining > 0 ? remaining : 0;
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!email() || !password()) { setError('请填写邮箱和密码'); return; }

    // 检查前端锁定
    const remaining = getRemainingLockSeconds();
    if (remaining > 0) {
      setError(`请等待 ${remaining} 秒后重试`);
      return;
    }

    setLoading(true);
    setError('');
    try {
      const res = await adminApi.login({ email: email(), password: password() });
      setPassword('');
      setFailCount(0);
      setLockUntil(0);
      tokenManager.setAdminToken(res.token);
      uiStore.toast.success('管理员登录成功');
      navigate('/admin', { replace: true });
    } catch (err: unknown) {
      const count = failCount() + 1;
      setFailCount(count);
      // 递增等待：第1次失败等1秒，第2次等2秒，依此类推，最多30秒
      const waitSeconds = Math.min(count, ADMIN_MAX_LOCK_WAIT_SECS);
      setLockUntil(Date.now() + waitSeconds * 1000);
      const msg = err instanceof Error ? err.message : '登录失败';
      setError(count >= 3 ? `${msg}（已失败 ${count} 次，请等待 ${waitSeconds} 秒）` : msg);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div class="min-h-screen flex items-center justify-center bg-surface-secondary p-4">
      <Card variant="elevated" class="w-full max-w-sm animate-fade-in-up">
        <div class="flex flex-col items-center gap-2 mb-6">
          <div class="w-12 h-12 rounded-full bg-accent flex items-center justify-center">
            <svg class="w-6 h-6 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
            </svg>
          </div>
          <p class="text-lg font-semibold text-content">WordForge Admin</p>
          <p class="text-sm text-content-tertiary">管理后台登录</p>
        </div>
        <form onSubmit={handleSubmit} class="space-y-4">
          <Input label="管理员邮箱" type="email" autocomplete="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
          <Input label="密码" type="password" autocomplete="current-password" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
          {error() && <p class="text-sm text-error text-center" role="alert">{error()}</p>}
          <Button type="submit" fullWidth loading={loading()}>登录</Button>
        </form>
      </Card>
    </div>
  );
}
