import { createSignal, createEffect } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { authStore } from '@/stores/auth';
import { uiStore } from '@/stores/ui';
import { LOGIN_THROTTLE_THRESHOLD, LOGIN_MAX_COOLDOWN_MS } from '@/lib/constants';

export default function LoginPage() {
  const navigate = useNavigate();

  // Redirect if already logged in (reactive)
  createEffect(() => {
    if (!authStore.loading() && authStore.isAuthenticated()) {
      navigate('/', { replace: true });
    }
  });

  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');
  const [failCount, setFailCount] = createSignal(0);
  const [cooldown, setCooldown] = createSignal(0);

  // 连续失败后增加提交间隔
  function getCooldownMs(failures: number): number {
    if (failures < LOGIN_THROTTLE_THRESHOLD) return 0;
    return Math.min(2 ** (failures - 2) * 1000, LOGIN_MAX_COOLDOWN_MS); // 3次失败=2s, 4次=4s, 5次=8s, 上限30s
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (cooldown() > 0) return;
    if (!email() || !password()) {
      setError('请填写邮箱和密码');
      return;
    }
    setLoading(true);
    setError('');
    try {
      await authStore.login(email(), password());
      setPassword('');
      setFailCount(0);
      uiStore.toast.success('登录成功');
      navigate('/', { replace: true });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : '登录失败';
      setError(msg);
      const newCount = failCount() + 1;
      setFailCount(newCount);
      const cd = getCooldownMs(newCount);
      if (cd > 0) {
        setCooldown(Math.ceil(cd / 1000));
        const timer = setInterval(() => {
          setCooldown((v) => {
            if (v <= 1) { clearInterval(timer); return 0; }
            return v - 1;
          });
        }, 1000);
      }
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
          {error() && <p class="text-sm text-error text-center" role="alert">{error()}</p>}
          <Button type="submit" fullWidth loading={loading()} disabled={cooldown() > 0}>
            {cooldown() > 0 ? `请等待 ${cooldown()} 秒` : '登录'}
          </Button>
        </form>
        <p class="mt-4 text-center text-sm text-content-secondary">
          还没有账号？ <A href="/register" class="text-accent hover:underline">立即注册</A>
        </p>
      </Card>
    </div>
  );
}
