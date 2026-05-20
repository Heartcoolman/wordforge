import { createSignal, onMount, onCleanup, Show } from 'solid-js';
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
  // 拆分：emailError 显示在邮箱框，formError 显示在表单顶部统一区
  const [emailError, setEmailError] = createSignal('');
  const [formError, setFormError] = createSignal('');
  const [failCount, setFailCount] = createSignal(0);
  const [lockUntil, setLockUntil] = createSignal(0);
  // setInterval 驱动倒计时，每秒重算一次
  const [lockSeconds, setLockSeconds] = createSignal(0);
  let lockTimer: ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    lockTimer = setInterval(() => {
      const remaining = Math.ceil((lockUntil() - Date.now()) / 1000);
      setLockSeconds(remaining > 0 ? remaining : 0);
    }, 1000);

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

  onCleanup(() => {
    if (lockTimer) clearInterval(lockTimer);
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    setEmailError('');
    setFormError('');
    if (!email()) { setEmailError('请填写邮箱'); return; }
    if (!password()) { setFormError('请填写密码'); return; }

    // 检查前端锁定
    if (lockSeconds() > 0) {
      setFormError(`请等待 ${lockSeconds()} 秒后重试`);
      return;
    }

    setLoading(true);
    try {
      const res = await adminApi.login({ email: email(), password: password() });
      setPassword('');
      setFailCount(0);
      setLockUntil(0);
      setLockSeconds(0);
      tokenManager.setAdminToken(res.token);
      uiStore.toast.success('管理员登录成功');
      navigate('/admin', { replace: true });
    } catch (err: unknown) {
      const count = failCount() + 1;
      setFailCount(count);
      // 递增等待：第1次失败等1秒，第2次等2秒，依此类推，最多30秒
      const waitSeconds = Math.min(count, ADMIN_MAX_LOCK_WAIT_SECS);
      setLockUntil(Date.now() + waitSeconds * 1000);
      setLockSeconds(waitSeconds);
      const msg = err instanceof Error ? err.message : '登录失败';
      setFormError(count >= 3 ? `${msg}（已失败 ${count} 次，请等待 ${waitSeconds} 秒）` : msg);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div
      class="relative min-h-screen flex items-center justify-center p-4 overflow-hidden bg-surface-secondary"
      style={{
        'padding-top': 'max(1rem, env(safe-area-inset-top))',
        'padding-bottom': 'max(1rem, env(safe-area-inset-bottom))',
        'padding-left': 'max(1rem, env(safe-area-inset-left))',
        'padding-right': 'max(1rem, env(safe-area-inset-right))',
      }}
    >
      {/* Hero 装饰：左上 accent 软光晕 + 右下 surface 渐变铺底 */}
      <div aria-hidden="true" class="absolute inset-0 bg-gradient-accent-soft pointer-events-none" />
      <div aria-hidden="true" class="absolute -top-32 -left-32 w-96 h-96 rounded-full bg-accent/10 blur-3xl pointer-events-none" />
      <div aria-hidden="true" class="absolute -bottom-32 -right-32 w-96 h-96 rounded-full bg-info/10 blur-3xl pointer-events-none" />

      <Card variant="elevated" class="relative w-full max-w-sm animate-fade-in-up shadow-elevation-4 border border-border-hairline">
        <div class="flex flex-col items-center gap-2 mb-6">
          <div class="w-12 h-12 rounded-full bg-gradient-accent-strong shadow-elevation-2 flex items-center justify-center animate-fade-in-up">
            <svg class="w-6 h-6 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
            </svg>
          </div>
          <p class="text-headline text-content animate-fade-in-up" style={{ 'animation-delay': '80ms', 'animation-fill-mode': 'backwards' }}>WordForge Admin</p>
          <p class="text-caption animate-fade-in-up" style={{ 'animation-delay': '160ms', 'animation-fill-mode': 'backwards' }}>管理后台登录</p>
        </div>
        <form onSubmit={handleSubmit} class="space-y-4">
          {/* 表单顶部统一错误条：承载登录失败 / 锁定提示，与字段级 emailError 分离 */}
          <Show when={formError()}>
            <div
              role="alert"
              class="rounded-md border border-error/30 bg-error-light px-3 py-2 text-sm text-error animate-fade-in"
            >
              {formError()}
            </div>
          </Show>
          <div class="animate-fade-in-up" style={{ 'animation-delay': '240ms', 'animation-fill-mode': 'backwards' }}>
            <Input
              label="管理员邮箱"
              type="email"
              required
              autocomplete="email"
              disabled={loading() || lockSeconds() > 0}
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
              error={emailError() || undefined}
            />
          </div>
          <div class="animate-fade-in-up" style={{ 'animation-delay': '320ms', 'animation-fill-mode': 'backwards' }}>
            <Input
              label="密码"
              type="password"
              required
              autocomplete="current-password"
              disabled={loading() || lockSeconds() > 0}
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
            />
          </div>
          <div class="animate-fade-in-up" style={{ 'animation-delay': '400ms', 'animation-fill-mode': 'backwards' }}>
            <Button type="submit" fullWidth loading={loading()} disabled={loading() || lockSeconds() > 0}>
              {lockSeconds() > 0 ? `锁定中 ${lockSeconds()}s` : '登录'}
            </Button>
          </div>
        </form>
      </Card>
    </div>
  );
}
