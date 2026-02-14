import { createSignal, createEffect, Show } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Card } from '@/components/ui/Card';
import { Modal } from '@/components/ui/Modal';
import { authStore } from '@/stores/auth';
import { authApi } from '@/api/auth';
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

  // 密码重置相关状态
  const [showResetModal, setShowResetModal] = createSignal(false);
  const [resetStep, setResetStep] = createSignal<'key' | 'password'>('key');
  const [resetKey, setResetKey] = createSignal('');
  const [newPassword, setNewPassword] = createSignal('');
  const [confirmPassword, setConfirmPassword] = createSignal('');
  const [resetLoading, setResetLoading] = createSignal(false);
  const [resetError, setResetError] = createSignal('');

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

  function resetModalState() {
    setResetStep('key');
    setResetKey('');
    setNewPassword('');
    setConfirmPassword('');
    setResetError('');
    setResetLoading(false);
  }

  async function handleVerifyKey(e: Event) {
    e.preventDefault();
    if (!resetKey().trim()) {
      setResetError('请输入密钥');
      return;
    }
    setResetLoading(true);
    setResetError('');
    try {
      const res = await authApi.verifyResetToken(resetKey().trim());
      if (!res.valid) {
        setResetError('密钥无效或已过期');
        return;
      }
      setResetStep('password');
    } catch (err: unknown) {
      setResetError(err instanceof Error ? err.message : '密钥无效或已过期');
    } finally {
      setResetLoading(false);
    }
  }

  async function handleResetSubmit(e: Event) {
    e.preventDefault();
    if (!newPassword()) {
      setResetError('请输入新密码');
      return;
    }
    if (newPassword() !== confirmPassword()) {
      setResetError('两次密码输入不一致');
      return;
    }
    setResetLoading(true);
    setResetError('');
    try {
      await authApi.resetPassword(resetKey().trim(), newPassword());
      setShowResetModal(false);
      resetModalState();
      uiStore.toast.success('密码重置成功，请用新密码登录');
    } catch (err: unknown) {
      setResetError(err instanceof Error ? err.message : '密码重置失败');
    } finally {
      setResetLoading(false);
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
        <p class="mt-2 text-center">
          <button
            type="button"
            class="text-sm text-content-tertiary hover:text-accent transition-colors cursor-pointer"
            onClick={() => { resetModalState(); setShowResetModal(true); }}
          >
            忘记密码？
          </button>
        </p>
      </Card>

      {/* 密码重置 Modal */}
      <Modal open={showResetModal()} onClose={() => { setShowResetModal(false); resetModalState(); }} title="重置密码" size="sm">
        <Show when={resetStep() === 'key'} fallback={
          <form onSubmit={handleResetSubmit} class="space-y-4 mt-2">
            <Input
              label="新密码"
              type="password"
              placeholder="输入新密码"
              value={newPassword()}
              onInput={(e) => setNewPassword(e.currentTarget.value)}
            />
            <Input
              label="确认密码"
              type="password"
              placeholder="再次输入新密码"
              value={confirmPassword()}
              onInput={(e) => setConfirmPassword(e.currentTarget.value)}
            />
            <Show when={resetError()}><p class="text-sm text-error text-center">{resetError()}</p></Show>
            <div class="flex justify-end gap-2 pt-2">
              <Button variant="ghost" onClick={() => { setResetStep('key'); setResetError(''); }}>上一步</Button>
              <Button type="submit" loading={resetLoading()}>重置密码</Button>
            </div>
          </form>
        }>
          <form onSubmit={handleVerifyKey} class="space-y-4 mt-2">
            <p class="text-sm text-content-secondary">请联系管理员获取密码重置密钥，然后在下方输入</p>
            <Input
              label="重置密钥"
              type="text"
              placeholder="输入密钥"
              value={resetKey()}
              onInput={(e) => setResetKey(e.currentTarget.value)}
            />
            <Show when={resetError()}><p class="text-sm text-error text-center">{resetError()}</p></Show>
            <div class="flex justify-end gap-2 pt-2">
              <Button variant="ghost" onClick={() => { setShowResetModal(false); resetModalState(); }}>取消</Button>
              <Button type="submit" loading={resetLoading()}>验证密钥</Button>
            </div>
          </form>
        </Show>
      </Modal>
    </div>
  );
}
