import { createSignal, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';

export default function AdminLoginPage() {
  const navigate = useNavigate();
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  onMount(async () => {
    // Check if admin is initialized
    try {
      const status = await adminApi.checkStatus();
      if (!status.initialized) {
        navigate('/admin/setup', { replace: true });
      }
    } catch { /* ignore */ }

    // If already logged in, redirect
    if (tokenManager.getAdminToken()) {
      navigate('/admin', { replace: true });
    }
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!email() || !password()) { setError('请填写邮箱和密码'); return; }
    setLoading(true);
    setError('');
    try {
      const res = await adminApi.login({ email: email(), password: password() });
      tokenManager.setAdminToken(res.token);
      uiStore.toast.success('管理员登录成功');
      navigate('/admin', { replace: true });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : '登录失败');
    } finally {
      setLoading(false);
    }
  }

  return (
    <div class="min-h-screen flex items-center justify-center bg-surface-secondary p-4">
      <Card variant="elevated" class="w-full max-w-sm animate-fade-in-up">
        <h1 class="text-2xl font-bold text-center text-content mb-6">管理后台登录</h1>
        <form onSubmit={handleSubmit} class="space-y-4">
          <Input label="管理员邮箱" type="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
          <Input label="密码" type="password" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
          {error() && <p class="text-sm text-error text-center">{error()}</p>}
          <Button type="submit" fullWidth loading={loading()}>登录</Button>
        </form>
      </Card>
    </div>
  );
}
