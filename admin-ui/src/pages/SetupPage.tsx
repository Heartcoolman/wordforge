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

export default function SetupPage() {
  const navigate = useNavigate();
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [confirm, setConfirm] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  // 拆分：confirmError 显示在确认密码框，formError 显示在表单顶部统一区
  const [confirmError, setConfirmError] = createSignal('');
  const [formError, setFormError] = createSignal('');
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
    setConfirmError('');
    setFormError('');
    if (!email() || !password()) { setFormError('请填写所有字段'); return; }
    if (password().length < MIN_PASSWORD_LENGTH) { setFormError(`密码至少 ${MIN_PASSWORD_LENGTH} 位`); return; }
    if (password() !== confirm()) { setConfirmError('密码不一致'); return; }
    setLoading(true);
    try {
      const res = await adminApi.setup({ email: email(), password: password() });
      setPassword('');
      setConfirm('');
      tokenManager.setAdminToken(res.token);
      uiStore.toast.success('管理员账户已创建');
      navigate('/admin', { replace: true });
    } catch (err: unknown) {
      setFormError(err instanceof Error ? err.message : '创建失败');
    } finally {
      setLoading(false);
    }
  }

  return (
    <div class="relative min-h-screen flex items-center justify-center p-4 overflow-hidden bg-surface-secondary">
      <div aria-hidden="true" class="absolute inset-0 bg-gradient-accent-soft pointer-events-none" />
      <div aria-hidden="true" class="absolute -top-32 -left-32 w-96 h-96 rounded-full bg-accent/10 blur-3xl pointer-events-none" />
      <div aria-hidden="true" class="absolute -bottom-32 -right-32 w-96 h-96 rounded-full bg-info/10 blur-3xl pointer-events-none" />

      <Show
        when={!checking()}
        fallback={
          <div class="flex flex-col items-center gap-2">
            <Spinner size="lg" />
            <p class="text-caption">正在检查后端…</p>
          </div>
        }
      >
        <Show when={!checkError()} fallback={
          <Card variant="elevated" class="relative w-full max-w-sm shadow-elevation-4 border border-border-hairline animate-fade-in-up">
            <Empty title="连接失败" description={checkError()} />
          </Card>
        }>
          <Card variant="elevated" class="relative w-full max-w-sm animate-fade-in-up shadow-elevation-4 border border-border-hairline">
            <h1 class="text-display text-content mb-2 text-center">初始化管理后台</h1>
            <p class="text-caption text-center mb-6">首次使用，请创建管理员账户</p>
            <form onSubmit={handleSubmit} class="space-y-4">
              <Show when={formError()}>
                <div
                  role="alert"
                  class="rounded-md border border-error/30 bg-error-light px-3 py-2 text-sm text-error animate-fade-in"
                >
                  {formError()}
                </div>
              </Show>
              <div class="animate-fade-in-up" style={{ 'animation-delay': '80ms', 'animation-fill-mode': 'backwards' }}>
                <Input label="管理员邮箱" type="email" required autocomplete="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
              </div>
              <div class="animate-fade-in-up" style={{ 'animation-delay': '160ms', 'animation-fill-mode': 'backwards' }}>
                <Input label="密码" type="password" required autocomplete="new-password" placeholder={`至少 ${MIN_PASSWORD_LENGTH} 位`} value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
              </div>
              <div class="animate-fade-in-up" style={{ 'animation-delay': '240ms', 'animation-fill-mode': 'backwards' }}>
                <Input label="确认密码" type="password" required autocomplete="new-password" value={confirm()} onInput={(e) => setConfirm(e.currentTarget.value)} error={confirmError() || undefined} />
              </div>
              <div class="animate-fade-in-up" style={{ 'animation-delay': '320ms', 'animation-fill-mode': 'backwards' }}>
                <Button type="submit" fullWidth loading={loading()} disabled={loading()}>创建管理员</Button>
              </div>
            </form>
          </Card>
        </Show>
      </Show>
    </div>
  );
}
