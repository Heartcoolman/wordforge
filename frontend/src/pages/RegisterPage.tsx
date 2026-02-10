import { createSignal } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { authStore } from '@/stores/auth';
import { uiStore } from '@/stores/ui';

export default function RegisterPage() {
  const navigate = useNavigate();

  // Redirect if already logged in
  if (authStore.isAuthenticated()) {
    navigate('/', { replace: true });
  }

  const [email, setEmail] = createSignal('');
  const [username, setUsername] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [confirm, setConfirm] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!email() || !username() || !password()) {
      setError('请填写所有字段');
      return;
    }
    if (password().length < 6) {
      setError('密码至少 6 位');
      return;
    }
    if (password() !== confirm()) {
      setError('两次密码不一致');
      return;
    }
    setLoading(true);
    setError('');
    try {
      await authStore.register(email(), username(), password());
      uiStore.toast.success('注册成功');
      navigate('/', { replace: true });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '注册失败';
      setError(msg);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div class="min-h-[70vh] flex items-center justify-center">
      <Card variant="elevated" class="w-full max-w-sm animate-fade-in-up">
        <h1 class="text-2xl font-bold text-center text-content mb-6">注册</h1>
        <form onSubmit={handleSubmit} class="space-y-4">
          <Input label="邮箱" type="email" placeholder="your@email.com" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
          <Input label="用户名" type="text" placeholder="昵称" value={username()} onInput={(e) => setUsername(e.currentTarget.value)} />
          <Input label="密码" type="password" placeholder="至少 6 位" value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
          <Input label="确认密码" type="password" placeholder="再次输入密码" value={confirm()} onInput={(e) => setConfirm(e.currentTarget.value)} />
          {error() && <p class="text-sm text-error text-center">{error()}</p>}
          <Button type="submit" fullWidth loading={loading()}>注册</Button>
        </form>
        <p class="mt-4 text-center text-sm text-content-secondary">
          已有账号？ <A href="/login" class="text-accent hover:underline">去登录</A>
        </p>
      </Card>
    </div>
  );
}
