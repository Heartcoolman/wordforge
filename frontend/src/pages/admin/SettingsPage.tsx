import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { Switch } from '@/components/ui/Switch';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import { SETTINGS_MAX_USERS, SETTINGS_MAX_DAILY_WORDS } from '@/lib/constants';

export default function SettingsPage() {
  const [settings, setSettings] = createSignal<{
    maxUsers: number;
    registrationEnabled: boolean;
    maintenanceMode: boolean;
    defaultDailyWords: number;
    wordbookCenterUrl?: string;
    amasAutoApplyEnabled: boolean;
    amasAutoApplyMaxPerDay: number;
    amasAutoApplyMinConfidence: number;
  } | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [broadcastTitle, setBroadcastTitle] = createSignal('');
  const [broadcastMsg, setBroadcastMsg] = createSignal('');
  const [broadcasting, setBroadcasting] = createSignal(false);
  const [showBroadcastConfirm, setShowBroadcastConfirm] = createSignal(false);
  const [updateMsg, setUpdateMsg] = createSignal('');
  const [sendingUpdate, setSendingUpdate] = createSignal(false);
  const [showUpdateConfirm, setShowUpdateConfirm] = createSignal(false);

  const [showMaintenanceConfirm, setShowMaintenanceConfirm] = createSignal(false);

  onMount(async () => {
    try {
      const s = await adminApi.getSettings();
      setSettings(s);
    } catch (e) {
      uiStore.toast.error('加载失败', e instanceof Error ? e.message : '未知错误');
    }
    setLoading(false);
  });

  async function saveSettings() {
    if (!settings()) return;
    const s = settings()!;
    // 范围校验
    if (s.maxUsers < 1 || s.maxUsers > SETTINGS_MAX_USERS) {
      uiStore.toast.warning(`最大用户数应在 1 ~ ${SETTINGS_MAX_USERS} 之间`);
      return;
    }
    if (s.defaultDailyWords < 1 || s.defaultDailyWords > SETTINGS_MAX_DAILY_WORDS) {
      uiStore.toast.warning(`默认每日单词数应在 1 ~ ${SETTINGS_MAX_DAILY_WORDS} 之间`);
      return;
    }
    setSaving(true);
    try {
      await adminApi.updateSettings(s);
      uiStore.toast.success('设置已保存');
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '');
    } finally {
      setSaving(false);
    }
  }

  function handleBroadcastClick() {
    if (!broadcastTitle().trim() || !broadcastMsg().trim()) {
      uiStore.toast.warning('请填写标题和内容');
      return;
    }
    setShowBroadcastConfirm(true);
  }

  async function confirmBroadcast() {
    setShowBroadcastConfirm(false);
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

  async function confirmUpdateBroadcast() {
    setShowUpdateConfirm(false);
    setSendingUpdate(true);
    try {
      await adminApi.broadcastUpdate(updateMsg().trim() ? { message: updateMsg() } : undefined);
      uiStore.toast.success('更新通知已发送');
      setUpdateMsg('');
    } catch (err: unknown) {
      uiStore.toast.error('发送失败', err instanceof Error ? err.message : '');
    } finally {
      setSendingUpdate(false);
    }
  }

  function handleMaintenanceToggle(value: boolean) {
    if (value) {
      setShowMaintenanceConfirm(true);
      return;
    }
    updateField('maintenanceMode', value);
  }

  function confirmMaintenance() {
    setShowMaintenanceConfirm(false);
    updateField('maintenanceMode', true);
  }

  function updateField(key: string, value: unknown) {
    setSettings((prev) => prev ? { ...prev, [key]: value } : prev);
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">系统设置</h1>

      {/* 广播确认弹窗 */}
      <Show when={showBroadcastConfirm()}>
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setShowBroadcastConfirm(false)}>
          <Card variant="elevated" class="max-w-sm mx-4" onClick={(e: MouseEvent) => e.stopPropagation()}>
            <h3 class="text-lg font-semibold text-content mb-2">确认发送广播</h3>
            <p class="text-sm text-content-secondary mb-2">
              标题: <span class="font-medium text-content">{broadcastTitle()}</span>
            </p>
            <p class="text-sm text-content-secondary mb-4">此消息将发送给所有用户，确认发送吗？</p>
            <div class="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setShowBroadcastConfirm(false)}>取消</Button>
              <Button size="sm" variant="warning" onClick={confirmBroadcast}>确认发送</Button>
            </div>
          </Card>
        </div>
      </Show>

      {/* 更新通知确认弹窗 */}
      <Show when={showUpdateConfirm()}>
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setShowUpdateConfirm(false)}>
          <Card variant="elevated" class="max-w-sm mx-4" onClick={(e: MouseEvent) => e.stopPropagation()}>
            <h3 class="text-lg font-semibold text-content mb-2">确认发送更新通知</h3>
            <p class="text-sm text-content-secondary mb-4">此通知将提示所有在线用户刷新页面获取新版本，确认发送吗？</p>
            <div class="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setShowUpdateConfirm(false)}>取消</Button>
              <Button size="sm" variant="warning" onClick={confirmUpdateBroadcast}>确认发送</Button>
            </div>
          </Card>
        </div>
      </Show>

      {/* 维护模式确认弹窗 */}
      <Show when={showMaintenanceConfirm()}>
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setShowMaintenanceConfirm(false)}>
          <Card variant="elevated" class="max-w-sm mx-4" onClick={(e: MouseEvent) => e.stopPropagation()}>
            <h3 class="text-lg font-semibold text-content mb-2">确认开启维护模式</h3>
            <p class="text-sm text-content-secondary mb-4">开启后所有非管理员用户将无法访问系统，确定要开启维护模式吗？</p>
            <div class="flex justify-end gap-2">
              <Button size="sm" variant="ghost" onClick={() => setShowMaintenanceConfirm(false)}>取消</Button>
              <Button size="sm" variant="warning" onClick={confirmMaintenance}>确认开启</Button>
            </div>
          </Card>
        </div>
      </Show>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Show when={settings()}>
          {(s) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-4">基本设置</h2>
              <div class="space-y-4">
                <Input
                  label="最大用户数"
                  type="number"
                  min={1}
                  max={100000}
                  value={String(s().maxUsers)}
                  onInput={(e) => updateField('maxUsers', parseInt(e.currentTarget.value) || 0)}
                />
                <Input
                  label="默认每日单词数"
                  type="number"
                  min={1}
                  max={500}
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
                  onChange={handleMaintenanceToggle}
                  label="维护模式"
                />
                <Input
                  label="词书中心 URL"
                  value={s().wordbookCenterUrl || ''}
                  onInput={(e) => updateField('wordbookCenterUrl', e.currentTarget.value || undefined)}
                  placeholder="https://cdn.example.com/wordbooks"
                />
                <div class="pt-2">
                  <Button onClick={saveSettings} loading={saving()}>保存设置</Button>
                </div>
              </div>
            </Card>
          )}
        </Show>

        <Show when={settings()}>
          {(s) => (
            <Card variant="elevated">
              <h2 class="text-lg font-semibold text-content mb-1">AMAS 调参自动化</h2>
              <p class="text-xs text-content-tertiary mb-4">
                启用后，由 LLM advisor 生成的建议如果满足白名单 + 单参 + 范围 + 置信度阈值 + 当日额度，
                将直接进入「已自动应用」状态并写入运行配置；否则仍走人工审批。
              </p>
              <div class="space-y-4">
                <Switch
                  checked={s().amasAutoApplyEnabled}
                  onChange={(v) => updateField('amasAutoApplyEnabled', v)}
                  label="启用灰度自动应用（默认关闭，强烈建议先观察建议-only 流）"
                />
                <Input
                  label="每日最多自动应用次数"
                  type="number"
                  min={0}
                  max={20}
                  value={String(s().amasAutoApplyMaxPerDay)}
                  onInput={(e) => updateField('amasAutoApplyMaxPerDay', parseInt(e.currentTarget.value) || 1)}
                  hint="超过此值的当日 patch 自动落 pending"
                />
                <Input
                  label="最低置信度阈值"
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={String(s().amasAutoApplyMinConfidence)}
                  onInput={(e) => updateField('amasAutoApplyMinConfidence', parseFloat(e.currentTarget.value) || 0.8)}
                  hint="LLM 自评 confidence 低于此值时自动转 pending"
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
            <Button onClick={handleBroadcastClick} loading={broadcasting()} variant="warning">发送广播</Button>
          </div>
        </Card>

        <Card variant="elevated">
          <h2 class="text-lg font-semibold text-content mb-4">更新通知</h2>
          <div class="space-y-3">
            <div class="flex flex-col gap-1.5">
              <label class="text-sm font-medium text-content-secondary">提示信息（可选）</label>
              <textarea
                class="w-full px-3 py-2 rounded-lg text-sm bg-surface text-content border border-border focus:outline-none focus:ring-2 focus:ring-accent/30 focus:border-accent resize-y min-h-[80px]"
                value={updateMsg()}
                onInput={(e) => setUpdateMsg(e.currentTarget.value)}
                placeholder="有新版本可用，请刷新页面获取最新内容"
              />
            </div>
            <Button onClick={() => setShowUpdateConfirm(true)} loading={sendingUpdate()} variant="warning">发送更新通知</Button>
          </div>
        </Card>
      </Show>
    </div>
  );
}
