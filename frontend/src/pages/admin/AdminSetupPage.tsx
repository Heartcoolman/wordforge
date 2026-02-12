import { createSignal, onMount, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { Empty } from '@/components/ui/Empty';
import { Spinner } from '@/components/ui/Spinner';
import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
import { uiStore } from '@/stores/ui';
import { MIN_PASSWORD_LENGTH } from '@/lib/constants';

export default function AdminSetupPage() {
  const navigate = useNavigate();
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [confirm, setConfirm] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');
  const [checking, setChecking] = createSignal(true);
  const [checkError, setCheckError] = createSignal('');

  onMount(async () => {
    try {
      const status = await adminApi.checkStatus();
      if (status.initialized) {
        navigate('/admin/login', { replace: true });
      }
    } catch (err: unknown) {
      setCheckError(err instanceof Error ? err.message : '无法连接到服务器，请检查后端是否运行');
    } finally {
      setChecking(false);
    }
  });

  async function handleSubmit(e: Event) {
    e.preventDefault();
    if (!email() || !password()) { setError('请填写所有字段'); return; }
    if (password() !== confirm()) { setError('密码不一致'); return; }
    if (password().length < MIN_PASSWORD_LENGTH) { setError(`密码至少 ${MIN_PASSWORD_LENGTH} 位`); return; }
    setLoading(true);
    setError('');
    try {
      const res = await adminApi.setup({ email: email(), password: password() });
      setPassword('');
      setConfirm('');
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
      <Show when={!checking()} fallback={<Spinner size="lg" />}>
        <Show when={!checkError()} fallback={
          <Card variant="elevated" class="w-full max-w-sm">
            <Empty title="连接失败" description={checkError()} />
          </Card>
        }>
          <Card variant="elevated" class="w-full max-w-sm animate-fade-in-up">
            <h1 class="text-2xl font-bold text-center text-content mb-2">初始化管理后台</h1>
            <p class="text-sm text-content-secondary text-center mb-6">首次使用，请创建管理员账户</p>
            <form onSubmit={handleSubmit} class="space-y-4">
              <Input label="管理员邮箱" type="email" autocomplete="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
              <Input label="密码" type="password" autocomplete="new-password" placeholder={`至少 ${MIN_PASSWORD_LENGTH} 位`} value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
              <Input label="确认密码" type="password" autocomplete="new-password" value={confirm()} onInput={(e) => setConfirm(e.currentTarget.value)} />
              {error() && <p class="text-sm text-error text-center" role="alert">{error()}</p>}
              <Button type="submit" fullWidth loading={loading()}>创建管理员</Button>
            </form>
          </Card>
        </Show>
      </Show>
    </div>
  );
}
