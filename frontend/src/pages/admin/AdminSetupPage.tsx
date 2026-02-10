import { createSignal } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';

export default function AdminSetupPage() {
  const navigate = useNavigate();
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [confirm, setConfirm] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!email() || !password()) { setError('请填写所有字段'); return; }
    if (password() !== confirm()) { setError('密码不一致'); return; }
    if (password().length < 6) { setError('密码至少 6 位'); return; }
    setLoading(true);
    setError('');
    try {
      const res = await adminApi.setup({ email: email(), password: password() });
      tokenManager.setAdminToken(res.token);
      uiStore.toast.success('管理员账户已创建');
      navigate('/admin', { replace: true });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : '创建失败');
    } finally {
      setLoading(false);
    }
  }

  return (
    <div class="min-h-screen flex items-center justify-center bg-surface-secondary p-4">
      <Card variant="elevated" class="w-full max-w-sm animate-fade-in-up">
        <h1 class="text-2xl font-bold text-center text-content mb-2">初始化管理后台</h1>
        <p class="text-sm text-content-secondary text-center mb-6">首次使用，请创建管理员账户</p>
        <form onSubmit={handleSubmit} class="space-y-4">
          <Input label="管理员邮箱" type="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
          <Input label="密码" type="password" placeholder="至少 6 位" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
          <Input label="确认密码" type="password" value={confirm()} onInput={(e) => setConfirm(e.currentTarget.value)} />
          {error() && <p class="text-sm text-error text-center">{error()}</p>}
          <Button type="submit" fullWidth loading={loading()}>创建管理员</Button>
        </form>
      </Card>
    </div>
  );
}
