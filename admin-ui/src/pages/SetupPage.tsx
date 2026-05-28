import { createSignal, createMemo, onMount, Show, For } from 'solid-js';
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

/**
 * 初始化向导。按设计图 setup.html 3 步 wizard：
 *   ① 环境检测 — ping /api/status，确保后端在线、未初始化
 *   ② 创建超管 — 邮箱 + 密码 + 二次确认（含密码强度校验）
 *   ③ 完成 — 显示成功 + 跳转登录按钮
 */

type Step = 1 | 2 | 3;
const STEP_LABELS: Record<Step, string> = {
  1: '环境检测',
  2: '创建超管',
  3: '登录',
};

export default function SetupPage() {
  const navigate = useNavigate();
  const [step, setStep] = createSignal<Step>(1);

  // Step 1 state
  const [checking, setChecking] = createSignal(true);
  const [checkError, setCheckError] = createSignal('');
  const [checkPassed, setCheckPassed] = createSignal(false);

  // Step 2 state
  const [email, setEmail] = createSignal('');
  const [password, setPassword] = createSignal('');
  const [confirm, setConfirm] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [confirmError, setConfirmError] = createSignal('');
  const [formError, setFormError] = createSignal('');

  onMount(async () => {
    try {
      const status = await adminApi.checkStatus();
      if (status.initialized) {
        navigate('/admin/login', { replace: true });
        return;
      }
      setCheckPassed(true);
    } catch (err: unknown) {
      setCheckError(err instanceof Error ? err.message : '无法连接到服务器，请检查后端是否运行');
    } finally {
      setChecking(false);
    }
  });

  async function retryCheck() {
    setChecking(true);
    setCheckError('');
    try {
      const status = await adminApi.checkStatus();
      if (status.initialized) {
        navigate('/admin/login', { replace: true });
        return;
      }
      setCheckPassed(true);
    } catch (err: unknown) {
      setCheckError(err instanceof Error ? err.message : '无法连接到服务器');
    } finally {
      setChecking(false);
    }
  }

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
      setStep(3);
    } catch (err: unknown) {
      setFormError(err instanceof Error ? err.message : '创建失败');
    } finally {
      setLoading(false);
    }
  }

  /**
   * 进度条状态：1=is-done(绿✓) / 2=is-current(accent) / 3=未到(灰)
   */
  const stepState = createMemo(() => (target: Step) => {
    const cur = step();
    if (target < cur) return 'is-done';
    if (target === cur) return 'is-current';
    return '';
  });

  return (
    <div class="relative min-h-screen flex items-center justify-center p-4 overflow-hidden bg-surface-secondary">
      <div aria-hidden="true" class="absolute inset-0 bg-gradient-accent-soft pointer-events-none" />
      <div aria-hidden="true" class="absolute -top-32 -left-32 size-96 rounded-full bg-accent/10 blur-3xl pointer-events-none" />
      <div aria-hidden="true" class="absolute -bottom-32 -right-32 size-96 rounded-full bg-info/10 blur-3xl pointer-events-none" />

      <Card variant="elevated" class="relative w-full max-w-md animate-fade-in-up shadow-elevation-4 border border-border-hairline">
        {/* Brand */}
        <div class="flex flex-col items-center gap-2 mb-5">
          <div class="size-12 rounded-xl bg-gradient-brand-mark shadow-elevation-2 grid place-items-center text-white font-bold text-xl tracking-[-0.04em] animate-scale-in">W</div>
          <h1 class="text-display text-content text-center">初始化管理后台</h1>
          <p class="text-caption text-center">3 步完成首次部署</p>
        </div>

        {/* Step progress */}
        <div class="flex items-center gap-2 justify-center mb-6 flex-wrap text-xs">
          <For each={[1, 2, 3] as Step[]}>
            {(s, i) => (
              <>
                <Show when={i() > 0}>
                  <span class="w-6 h-px bg-border" />
                </Show>
                <span
                  class={
                    stepState()(s) === 'is-done'
                      ? 'inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-success-light text-success-strong font-medium'
                      : stepState()(s) === 'is-current'
                        ? 'inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-accent-light text-accent font-medium'
                        : 'inline-flex items-center gap-1 px-2.5 py-1 rounded-full bg-surface-secondary text-content-tertiary'
                  }
                >
                  <Show when={stepState()(s) === 'is-done'} fallback={<span class="tabular-nums">{s}</span>}>
                    <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3">
                      <path d="M5 13l4 4L19 7" />
                    </svg>
                  </Show>
                  {STEP_LABELS[s]}
                </span>
              </>
            )}
          </For>
        </div>

        {/* Step 1: 环境检测 */}
        <Show when={step() === 1}>
          <div class="animate-tab-content-slide space-y-4">
            <Show
              when={!checking()}
              fallback={
                <div class="flex flex-col items-center gap-2 py-8">
                  <Spinner size="lg" />
                  <p class="text-caption">检测后端服务…</p>
                </div>
              }
            >
              <Show when={checkError()} fallback={
                <div class="flex flex-col items-center gap-3 py-4">
                  <div class="size-12 rounded-full bg-success-light text-success-strong grid place-items-center">
                    <svg class="w-6 h-6" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                      <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                    </svg>
                  </div>
                  <p class="text-headline text-content">后端在线，未初始化</p>
                  <p class="text-sm text-content-secondary text-center">可以继续创建管理员账户。</p>
                  <Button fullWidth onClick={() => setStep(2)}>下一步：创建超管</Button>
                </div>
              }>
                <Empty title="连接失败" description={checkError()} />
                <Button fullWidth variant="outline" onClick={retryCheck}>重试</Button>
              </Show>
            </Show>
          </div>
        </Show>

        {/* Step 2: 创建超管 */}
        <Show when={step() === 2 && checkPassed()}>
          <form onSubmit={handleSubmit} class="space-y-4 animate-tab-content-slide">
            <Show when={formError()}>
              <div role="alert" class="rounded-md border border-error/30 bg-error-light px-3 py-2 text-sm text-error animate-fade-in">
                {formError()}
              </div>
            </Show>
            <Input label="管理员邮箱" type="email" required autocomplete="email" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
            <Input label="密码" type="password" required autocomplete="new-password" placeholder={`至少 ${MIN_PASSWORD_LENGTH} 位`} value={password()} onInput={(e) => setPassword(e.currentTarget.value)} />
            <Input label="确认密码" type="password" required autocomplete="new-password" value={confirm()} onInput={(e) => setConfirm(e.currentTarget.value)} error={confirmError() || undefined} />
            <div class="flex items-center gap-2">
              <Button variant="ghost" onClick={() => setStep(1)} disabled={loading()}>上一步</Button>
              <Button type="submit" fullWidth loading={loading()} disabled={loading()}>创建管理员</Button>
            </div>
          </form>
        </Show>

        {/* Step 3: 完成 */}
        <Show when={step() === 3}>
          <div class="flex flex-col items-center gap-3 py-4 animate-tab-content-slide">
            <div class="size-14 rounded-full bg-success-light text-success-strong grid place-items-center animate-scale-in">
              <svg class="w-7 h-7" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
              </svg>
            </div>
            <p class="text-headline text-content">初始化完成</p>
            <p class="text-sm text-content-secondary text-center">管理员账户已创建并自动登录。点击下面按钮进入后台。</p>
            <Button fullWidth onClick={() => navigate('/admin', { replace: true })}>进入后台</Button>
          </div>
        </Show>

        {/* Footer */}
        <p class="mt-6 pt-4 border-t border-border-hairline text-[11px] text-content-tertiary text-center font-mono">
          WordForge Admin · 初始化向导
        </p>
      </Card>
    </div>
  );
}
