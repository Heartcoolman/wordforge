import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Switch } from '@/components/ui/Switch';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';

export default function SettingsPage() {
  const [settings, setSettings] = createSignal<{
    maxUsers: number;
    registrationEnabled: boolean;
    maintenanceMode: boolean;
    defaultDailyWords: number;
  } | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [broadcastTitle, setBroadcastTitle] = createSignal('');
  const [broadcastMsg, setBroadcastMsg] = createSignal('');
  const [broadcasting, setBroadcasting] = createSignal(false);

  onMount(async () => {
    try {
      const s = await adminApi.getSettings();
      setSettings(s);
    } catch { /* ignore */ }
    setLoading(false);
  });

  async function saveSettings() {
    if (!settings()) return;
    setSaving(true);
    try {
      await adminApi.updateSettings(settings()!);
      uiStore.toast.success('设置已保存');
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '');
    } finally {
      setSaving(false);
    }
  }

  async function sendBroadcast() {
    if (!broadcastTitle().trim() || !broadcastMsg().trim()) {
      uiStore.toast.warning('请填写标题和内容');
      return;
    }
    setBroadcasting(true);
    try {
      const res = await adminApi.broadcast({ title: broadcastTitle(), message: broadcastMsg() });
      uiStore.toast.success(`已发送给 ${res.sent} 位用户`);
      setBroadcastTitle('');
      setBroadcastMsg('');
    } catch (err: unknown) {
      uiStore.toast.error('发送失败', err instanceof Error ? err.message : '');
    } finally {
      setBroadcasting(false);
    }
  }

  function updateField(key: string, value: unknown) {
    setSettings((prev) => prev ? { ...prev, [key]: value } : prev);
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">系统设置</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={settings()}>
          {(s) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-4">基本设置</h2>
              <div class="space-y-4">
                <Input
                  label="最大用户数"
                  type="number"
                  value={String(s().maxUsers)}
                  onInput={(e) => updateField('maxUsers', parseInt(e.currentTarget.value) || 0)}
                />
                <Input
                  label="默认每日单词数"
                  type="number"
                  value={String(s().defaultDailyWords)}
                  onInput={(e) => updateField('defaultDailyWords', parseInt(e.currentTarget.value) || 20)}
                />
                <Switch
                  checked={s().registrationEnabled}
                  onChange={(v) => updateField('registrationEnabled', v)}
                  label="开放注册"
                />
                <Switch
                  checked={s().maintenanceMode}
                  onChange={(v) => updateField('maintenanceMode', v)}
                  label="维护模式"
                />
                <div class="pt-2">
                  <Button onClick={saveSettings} loading={saving()}>保存设置</Button>
                </div>
              </div>
            </Card>
          )}
        </Show>

        <Card variant="elevated">
          <h2 class="text-lg font-semibold text-content mb-4">广播消息</h2>
          <div class="space-y-3">
            <Input label="标题" value={broadcastTitle()} onInput={(e) => setBroadcastTitle(e.currentTarget.value)} placeholder="通知标题" />
            <div class="flex flex-col gap-1.5">
              <label class="text-sm font-medium text-content-secondary">内容</label>
              <textarea
                class="w-full px-3 py-2 rounded-lg text-sm bg-surface text-content border border-border focus:outline-none focus:ring-2 focus:ring-accent/30 focus:border-accent resize-y min-h-[80px]"
                value={broadcastMsg()}
                onInput={(e) => setBroadcastMsg(e.currentTarget.value)}
                placeholder="通知内容"
              />
            </div>
            <Button onClick={sendBroadcast} loading={broadcasting()} variant="warning">发送广播</Button>
          </div>
        </Card>
      </Show>
    </div>
  );
}
