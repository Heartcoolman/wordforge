import { createSignal } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { authStore } from '@/stores/auth';
import { uiStore } from '@/stores/ui';

export default function LoginPage() {
  const navigate = useNavigate();

  // Redirect if already logged in
  if (authStore.isAuthenticated()) {
    navigate('/', { replace: true });
  }

  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!email() || !password()) {
      setError('请填写邮箱和密码');
      return;
    }
    setLoading(true);
    setError('');
    try {
      await authStore.login(email(), password());
      uiStore.toast.success('登录成功');
      navigate('/', { replace: true });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '登录失败';
      setError(msg);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div class="min-h-[70vh] flex items-center justify-center">
      <Card variant="elevated" class="w-full max-w-sm animate-fade-in-up">
        <h1 class="text-2xl font-bold text-center text-content mb-6">登录</h1>
        <form onSubmit={handleSubmit} class="space-y-4">
          <Input
            label="邮箱"
            type="email"
            placeholder="your@email.com"
            value={email()}
            onInput={(e) => setEmail(e.currentTarget.value)}
            error={error() && !email() ? '请输入邮箱' : undefined}
          />
          <Input
            label="密码"
            type="password"
            placeholder="输入密码"
            value={password()}
            onInput={(e) => setPassword(e.currentTarget.value)}
          />
          {error() && <p class="text-sm text-error text-center">{error()}</p>}
          <Button type="submit" fullWidth loading={loading()}>
            登录
          </Button>
        </form>
        <p class="mt-4 text-center text-sm text-content-secondary">
          还没有账号？ <A href="/register" class="text-accent hover:underline">立即注册</A>
        </p>
      </Card>
    </div>
  );
}
