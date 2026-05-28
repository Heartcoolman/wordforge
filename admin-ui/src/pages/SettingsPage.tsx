import { createSignal, Show, onMount, onCleanup, createMemo } from 'solid-js';
import { A } from '@solidjs/router';
import { Card } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
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
  // 广播 / 更新通知模块已迁出到 /admin/broadcast（独立路由，三类广播 + 模板 + 历史）

  const [showMaintenanceConfirm, setShowMaintenanceConfirm] = createSignal(false);
  const [savingMaintenance, setSavingMaintenance] = createSignal(false);

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

  function handleMaintenanceToggle(value: boolean) {
    if (value) {
      setShowMaintenanceConfirm(true);
      return;
    }
    void applyMaintenance(false);
  }

  async function applyMaintenance(active: boolean) {
    setSavingMaintenance(true);
    try {
      await adminApi.setMaintenance(active);
      updateField('maintenanceMode', active);
      setBaseline((b) => b ? { ...b, maintenanceMode: active } : b);
      uiStore.toast.success(active ? '维护模式已开启' : '维护模式已关闭');
    } catch (err: unknown) {
      uiStore.toast.error('切换失败', err instanceof Error ? err.message : '');
    } finally {
      setSavingMaintenance(false);
    }
  }

  function confirmMaintenance() {
    setShowMaintenanceConfirm(false);
    void applyMaintenance(true);
  }

  function updateField(key: string, value: unknown) {
    setSettings((prev) => prev ? { ...prev, [key]: value } : prev);
  }

  return (
    <div class="space-y-6">
      <h1 class="text-title text-content">系统设置</h1>

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
                  disabled={savingMaintenance()}
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

        {/* 广播 / 更新通知模块已迁出到独立路由 */}
        <Card variant="outlined">
          <div class="flex items-start gap-3">
            <div class="size-9 rounded-lg bg-info-light text-info-strong grid place-items-center shrink-0">
              <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M11 5.882V19.24a1.76 1.76 0 01-3.417.592l-2.147-6.15M18 13a3 3 0 100-6M5.436 13.683A4.001 4.001 0 017 6h1.832c4.1 0 7.625-1.234 9.168-3v14c-1.543-1.766-5.067-3-9.168-3H7a3.988 3.988 0 01-1.564-.317z" />
              </svg>
            </div>
            <div class="flex-1">
              <h3 class="text-headline mb-1">广播 / 更新通知已迁出</h3>
              <p class="text-sm text-content-secondary mb-3">
                广播管理已移至独立的「系统广播」页（三类广播 + 模板库 + 发送历史 + 送达统计）。
              </p>
              <A href="/admin/broadcast" class="inline-flex items-center gap-1.5 text-accent text-sm font-medium hover:underline">
                前往系统广播
                <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path d="M5 12h14M13 6l6 6-6 6" /></svg>
              </A>
            </div>
          </div>
        </Card>
      </Show>
    </div>
  );
}
