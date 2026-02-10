import { createSignal, Show, onMount } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { authStore } from '@/stores/auth';
import { uiStore } from '@/stores/ui';
import { usersApi } from '@/api/users';

export default function ProfilePage() {
  const navigate = useNavigate();
  const [username, setUsername] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  const [currentPw, setCurrentPw] = createSignal('');
  const [newPw, setNewPw] = createSignal('');
  const [confirmPw, setConfirmPw] = createSignal('');
  const [changingPw, setChangingPw] = createSignal(false);

  onMount(() => {
    const u = authStore.user();
    if (u) setUsername(u.username);
  });

  async function saveUsername() {
    if (!username().trim()) return;
    setSaving(true);
    try {
      const updated = await usersApi.updateMe({ username: username() });
      authStore.updateUser(updated);
      uiStore.toast.success('用户名已更新');
    } catch (err: unknown) {
      uiStore.toast.error('更新失败', err instanceof Error ? err.message : '');
    } finally {
      setSaving(false);
    }
  }

  async function changePassword() {
    if (!currentPw() || !newPw()) return;
    if (newPw().length < 6) { uiStore.toast.error('新密码至少 6 位'); return; }
    if (newPw() !== confirmPw()) { uiStore.toast.error('两次密码不一致'); return; }
    setChangingPw(true);
    try {
      await usersApi.changePassword({ current_password: currentPw(), new_password: newPw() });
      uiStore.toast.success('密码已修改');
      setCurrentPw(''); setNewPw(''); setConfirmPw('');
    } catch (err: unknown) {
      uiStore.toast.error('修改失败', err instanceof Error ? err.message : '');
    } finally {
      setChangingPw(false);
    }
  }

  async function handleLogout() {
    await authStore.logout();
    navigate('/login', { replace: true });
  }

  return (
    <div class="max-w-lg mx-auto space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">个人中心</h1>

      <Show when={authStore.user()}>
        {(user) => (
          <Card variant="elevated">
            <div class="space-y-4">
              <div>
                <p class="text-sm text-content-secondary">邮箱</p>
                <p class="text-content font-medium">{user().email}</p>
              </div>
              <Input label="用户名" value={username()} onInput={(e) => setUsername(e.currentTarget.value)} />
              <Button onClick={saveUsername} loading={saving()} size="sm">保存</Button>
            </div>
          </Card>
        )}
      </Show>

      <Card variant="elevated">
        <h2 class="text-lg font-semibold text-content mb-4">修改密码</h2>
        <div class="space-y-3">
          <Input label="当前密码" type="password" value={currentPw()} onInput={(e) => setCurrentPw(e.currentTarget.value)} />
          <Input label="新密码" type="password" placeholder="至少 6 位" value={newPw()} onInput={(e) => setNewPw(e.currentTarget.value)} />
          <Input label="确认新密码" type="password" placeholder="再次输入新密码" value={confirmPw()} onInput={(e) => setConfirmPw(e.currentTarget.value)} />
          <Button onClick={changePassword} loading={changingPw()} size="sm" variant="secondary">修改密码</Button>
        </div>
      </Card>

      <Card variant="outlined">
        <Button onClick={handleLogout} variant="danger" fullWidth>退出登录</Button>
      </Card>
    </div>
  );
}
