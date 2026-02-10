import { createSignal, Show, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { uiStore } from '@/stores/ui';
import { amasApi } from '@/api/amas';

export default function AmasConfigPage() {
  const [config, setConfig] = createSignal('');
  const [metrics, setMetrics] = createSignal<unknown>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);

  onMount(async () => {
    try {
      const [c, m] = await Promise.allSettled([amasApi.getConfig(), amasApi.getMetrics()]);
      if (c.status === 'fulfilled') setConfig(JSON.stringify(c.value, null, 2));
      if (m.status === 'fulfilled') setMetrics(m.value);
    } catch { /* ignore */ }
    setLoading(false);
  });

  async function saveConfig() {
    try {
      const parsed = JSON.parse(config());
      setSaving(true);
      await amasApi.updateConfig(parsed);
      uiStore.toast.success('AMAS 配置已更新');
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : 'JSON 格式错误');
    } finally {
      setSaving(false);
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
          />
          <div class="flex justify-end mt-3">
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
