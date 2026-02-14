import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { amasApi } from '@/api/amas';
import { adminApi } from '@/api/admin';
import type { AmasConfig } from '@/types/amas';

export default function AmasConfigPage() {
  const [config, setConfig] = createSignal('');
  const [metrics, setMetrics] = createSignal<unknown>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [reloading, setReloading] = createSignal(false);

  onMount(async () => {
    const [c, m] = await Promise.allSettled([amasApi.getConfig(), amasApi.getMetrics()]);
    if (c.status === 'fulfilled') setConfig(JSON.stringify(c.value, null, 2));
    if (m.status === 'fulfilled') setMetrics(m.value);
    setLoading(false);
  });

  function parseConfigInput() {
    let parsed: unknown;
    try {
      parsed = JSON.parse(config());
    } catch {
      uiStore.toast.error('保存失败', 'JSON 格式错误，请检查语法');
      return null;
    }

    // 基本 schema 校验：配置必须是一个非空对象
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      uiStore.toast.error('保存失败', '配置必须是一个 JSON 对象');
      return null;
    }
    return parsed as AmasConfig;
  }

  async function saveConfig() {
    const parsed = parseConfigInput();
    if (!parsed) return;
    try {
      setSaving(true);
      await amasApi.updateConfig(parsed);
      uiStore.toast.success('AMAS 配置已更新');
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setSaving(false);
    }
  }

  async function reloadAmasConfig() {
    const parsed = parseConfigInput();
    if (!parsed) return;
    try {
      setReloading(true);
      const latest = await adminApi.reloadAmas(parsed);
      setConfig(JSON.stringify(latest, null, 2));
      uiStore.toast.success('AMAS 配置已热重载');
    } catch (err: unknown) {
      uiStore.toast.error('热重载失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setReloading(false);
    }
  }

  return (
    <div class="space-y-6 animate-fade-in-up">
      <h1 class="text-2xl font-bold text-content">AMAS 配置</h1>

      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Card variant="elevated">
          <h2 class="text-lg font-semibold text-content mb-3">配置编辑器</h2>
          <textarea
            class="w-full h-80 px-4 py-3 rounded-lg text-sm font-mono bg-surface border border-border text-content focus:outline-none focus:ring-2 focus:ring-accent/30 focus:border-accent resize-y"
            value={config()}
            onInput={(e) => setConfig(e.currentTarget.value)}
            spellcheck={false}
          />
          <p class="text-xs text-content-tertiary mt-1">请输入合法的 JSON 对象，例如 {"{"} "key": "value" {"}"}</p>
          <div class="flex justify-end gap-2 mt-3">
            <Button onClick={reloadAmasConfig} loading={reloading()} variant="ghost">热重载配置</Button>
            <Button onClick={saveConfig} loading={saving()}>保存配置</Button>
          </div>
        </Card>

        <Show when={metrics()}>
          <Card variant="elevated">
            <h2 class="text-lg font-semibold text-content mb-3">算法指标</h2>
            <pre class="text-xs font-mono text-content-secondary bg-surface-secondary p-4 rounded-lg overflow-x-auto">
              {JSON.stringify(metrics(), null, 2)}
            </pre>
          </Card>
        </Show>
      </Show>
    </div>
  );
}
