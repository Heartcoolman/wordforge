import { createSignal, Show, onMount, onCleanup, createMemo } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Input, TextArea } from '@/components/ui/Input';
import { Button } from '@/components/ui/Button';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { Switch } from '@/components/ui/Switch';
import { Skeleton } from '@/components/ui/Skeleton';
import { uiStore } from '@/stores/ui';
import { adminApi } from '@/api/admin';
import { SETTINGS_MAX_USERS, SETTINGS_MAX_DAILY_WORDS } from '@/lib/constants';

type SettingsShape = {
  maxUsers: number;
  registrationEnabled: boolean;
  maintenanceMode: boolean;
  defaultDailyWords: number;
  wordbookCenterUrl?: string;
  amasAutoApplyEnabled: boolean;
  amasAutoApplyMaxPerDay: number;
  amasAutoApplyMinConfidence: number;
};

export default function SettingsPage() {
  const [settings, setSettings] = createSignal<SettingsShape | null>(null);
  const [baseline, setBaseline] = createSignal<SettingsShape | null>(null);
  const [loading, setLoading] = createSignal(true);
  // 拆分基本/AMAS 两块的 saving 标志，避免共用一个 signal 同时 spin
  const [savingBasic, setSavingBasic] = createSignal(false);
  const [savingAmas, setSavingAmas] = createSignal(false);
  const [broadcastTitle, setBroadcastTitle] = createSignal('');
  const [broadcastMsg, setBroadcastMsg] = createSignal('');
  const [broadcasting, setBroadcasting] = createSignal(false);
  const [showBroadcastConfirm, setShowBroadcastConfirm] = createSignal(false);
  const [updateMsg, setUpdateMsg] = createSignal('');
  const [sendingUpdate, setSendingUpdate] = createSignal(false);
  const [showUpdateConfirm, setShowUpdateConfirm] = createSignal(false);

  const [showMaintenanceConfirm, setShowMaintenanceConfirm] = createSignal(false);

  // 数字字段 string 缓存：避免清空时被 || 默认值立即回填
  const [maxUsersInput, setMaxUsersInput] = createSignal('');
  const [defaultDailyWordsInput, setDefaultDailyWordsInput] = createSignal('');
  const [maxPerDayInput, setMaxPerDayInput] = createSignal('');
  const [minConfidenceInput, setMinConfidenceInput] = createSignal('');

  const isDirty = createMemo(() => {
    const cur = settings();
    const base = baseline();
    if (!cur || !base) return false;
    return JSON.stringify(cur) !== JSON.stringify(base);
  });

  function beforeUnloadHandler(e: BeforeUnloadEvent) {
    if (isDirty()) {
      e.preventDefault();
      // Chrome 需要 returnValue 才会弹原生确认
      e.returnValue = '';
    }
  }

  onMount(async () => {
    try {
      const s = await adminApi.getSettings();
      setSettings(s);
      setBaseline({ ...s });
      // 同步 string 缓存
      setMaxUsersInput(String(s.maxUsers));
      setDefaultDailyWordsInput(String(s.defaultDailyWords));
      setMaxPerDayInput(String(s.amasAutoApplyMaxPerDay));
      setMinConfidenceInput(String(s.amasAutoApplyMinConfidence));
    } catch (e) {
      uiStore.toast.error('加载失败', e instanceof Error ? e.message : '未知错误');
    }
    setLoading(false);
    window.addEventListener('beforeunload', beforeUnloadHandler);
  });

  onCleanup(() => {
    window.removeEventListener('beforeunload', beforeUnloadHandler);
  });

  // 将当前 string 缓存合并回 settings 后再校验
  function resolveSettingsForSave(): SettingsShape | null {
    const cur = settings();
    if (!cur) return null;
    return {
      ...cur,
      maxUsers: Number(maxUsersInput()) || cur.maxUsers,
      defaultDailyWords: Number(defaultDailyWordsInput()) || cur.defaultDailyWords,
      amasAutoApplyMaxPerDay: Number(maxPerDayInput()) || cur.amasAutoApplyMaxPerDay,
      amasAutoApplyMinConfidence: Number(minConfidenceInput()) || cur.amasAutoApplyMinConfidence,
    };
  }

  async function saveSettings(section: 'basic' | 'amas') {
    const merged = resolveSettingsForSave();
    if (!merged) return;
    // 范围校验
    if (merged.maxUsers < 1 || merged.maxUsers > SETTINGS_MAX_USERS) {
      uiStore.toast.warning(`最大用户数应在 1 ~ ${SETTINGS_MAX_USERS} 之间`);
      return;
    }
    if (merged.defaultDailyWords < 1 || merged.defaultDailyWords > SETTINGS_MAX_DAILY_WORDS) {
      uiStore.toast.warning(`默认每日单词数应在 1 ~ ${SETTINGS_MAX_DAILY_WORDS} 之间`);
      return;
    }
    const setSaving = section === 'basic' ? setSavingBasic : setSavingAmas;
    setSaving(true);
    try {
      await adminApi.updateSettings(merged);
      setSettings(merged);
      setBaseline({ ...merged });
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
    <div class="space-y-6">
      <h1 class="text-title text-content">系统设置</h1>

      {/* 广播确认弹窗 */}
      <ConfirmDialog
        open={showBroadcastConfirm()}
        title="确认发送广播"
        message={
          <>
            <p class="mb-2">标题: <span class="font-medium text-content">{broadcastTitle()}</span></p>
            <p>此消息将发送给所有用户，确认发送吗？</p>
          </>
        }
        confirmText="确认发送"
        variant="warning"
        onConfirm={confirmBroadcast}
        onCancel={() => setShowBroadcastConfirm(false)}
      />

      {/* 更新通知确认弹窗 */}
      <ConfirmDialog
        open={showUpdateConfirm()}
        title="确认发送更新通知"
        message="此通知将提示所有在线用户刷新页面获取新版本，确认发送吗？"
        confirmText="确认发送"
        variant="warning"
        onConfirm={confirmUpdateBroadcast}
        onCancel={() => setShowUpdateConfirm(false)}
      />

      {/* 维护模式确认弹窗 */}
      <ConfirmDialog
        open={showMaintenanceConfirm()}
        title="确认开启维护模式"
        message="开启后所有非管理员用户将无法访问系统，确定要开启维护模式吗？"
        confirmText="确认开启"
        variant="warning"
        onConfirm={confirmMaintenance}
        onCancel={() => setShowMaintenanceConfirm(false)}
      />

      <Show
        when={!loading()}
        fallback={
          <div class="space-y-3">
            <Skeleton height="2.5rem" />
            <Skeleton height="2.5rem" />
            <Skeleton height="2.5rem" />
            <Skeleton height="2.5rem" />
          </div>
        }
      >
        <Show
          when={settings()}
          fallback={
            <div class="space-y-3">
              <Skeleton height="2rem" />
              <Skeleton height="2rem" />
            </div>
          }
        >
          {(s) => (
            <Card variant="elevated">
              <h2 class="text-headline text-content mb-4">基本设置</h2>
              <div class="space-y-4">
                <Input
                  label="最大用户数"
                  type="number"
                  min={1}
                  max={100000}
                  value={maxUsersInput()}
                  onInput={(e) => setMaxUsersInput(e.currentTarget.value)}
                />
                <Input
                  label="默认每日单词数"
                  type="number"
                  min={1}
                  max={500}
                  value={defaultDailyWordsInput()}
                  onInput={(e) => setDefaultDailyWordsInput(e.currentTarget.value)}
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
                  <Button onClick={() => saveSettings('basic')} loading={savingBasic()} disabled={savingBasic()}>
                    保存设置
                  </Button>
                </div>
              </div>
            </Card>
          )}
        </Show>

        <Show when={settings()}>
          {(s) => (
            <Card variant="elevated">
              <h2 class="text-headline text-content mb-1">AMAS 调参自动化</h2>
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
                  value={maxPerDayInput()}
                  onInput={(e) => setMaxPerDayInput(e.currentTarget.value)}
                  hint="超过此值的当日 patch 自动落 pending"
                />
                <Input
                  label="最低置信度阈值"
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={minConfidenceInput()}
                  onInput={(e) => setMinConfidenceInput(e.currentTarget.value)}
                  hint="LLM 自评 confidence 低于此值时自动转 pending"
                />
                <div class="pt-2">
                  <Button onClick={() => saveSettings('amas')} loading={savingAmas()} disabled={savingAmas()}>
                    保存设置
                  </Button>
                </div>
              </div>
            </Card>
          )}
        </Show>

        <Card variant="elevated">
          <h2 class="text-headline text-content mb-4">广播消息</h2>
          <div class="space-y-4">
            <Input
              label="标题"
              value={broadcastTitle()}
              disabled={broadcasting()}
              onInput={(e) => setBroadcastTitle(e.currentTarget.value)}
              placeholder="通知标题"
            />
            <TextArea
              label="内容"
              value={broadcastMsg()}
              disabled={broadcasting()}
              onInput={(e) => setBroadcastMsg(e.currentTarget.value)}
              placeholder="通知内容"
            />
            <Button
              onClick={handleBroadcastClick}
              loading={broadcasting()}
              disabled={broadcasting() || showBroadcastConfirm()}
              variant="warning"
            >
              发送广播
            </Button>
          </div>
        </Card>

        <Card variant="elevated">
          <h2 class="text-headline text-content mb-4">更新通知</h2>
          <div class="space-y-4">
            <TextArea
              label="提示信息（可选）"
              value={updateMsg()}
              disabled={sendingUpdate()}
              onInput={(e) => setUpdateMsg(e.currentTarget.value)}
              placeholder="有新版本可用，请刷新页面获取最新内容"
            />
            <Button
              onClick={() => setShowUpdateConfirm(true)}
              loading={sendingUpdate()}
              disabled={sendingUpdate() || showUpdateConfirm()}
              variant="warning"
            >
              发送更新通知
            </Button>
          </div>
        </Card>
      </Show>
    </div>
  );
}
