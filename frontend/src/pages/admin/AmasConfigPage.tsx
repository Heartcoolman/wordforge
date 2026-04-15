import { createSignal, Show, For, onMount } from 'solid-js';
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
          {(m) => {
            const entries = () => Object.entries(m() as Record<string, { callCount: number; totalLatencyUs: number; errorCount: number }>);
            return (
              <Card variant="elevated">
                <h2 class="text-lg font-semibold text-content mb-3">算法指标</h2>
                <Show when={entries().length > 0} fallback={<p class="text-sm text-content-secondary">暂无指标数据</p>}>
                  <div class="overflow-x-auto">
                    <table class="w-full text-sm">
                      <thead>
                        <tr class="bg-surface-secondary border-b border-border">
                          <th class="px-4 py-2 text-left font-medium text-content-secondary">算法名称</th>
                          <th class="px-4 py-2 text-right font-medium text-content-secondary">调用次数</th>
                          <th class="px-4 py-2 text-right font-medium text-content-secondary">平均延迟</th>
                          <th class="px-4 py-2 text-right font-medium text-content-secondary">错误次数</th>
                        </tr>
                      </thead>
                      <tbody>
                        <For each={entries()}>
                          {([name, snapshot]) => (
                            <tr class="border-b border-border/50 hover:bg-surface-secondary/50 transition-colors">
                              <td class="px-4 py-2 font-mono text-sm">{name}</td>
                              <td class="px-4 py-2 text-right">{snapshot.callCount}</td>
                              <td class="px-4 py-2 text-right">
                                {snapshot.callCount > 0 ? `${(snapshot.totalLatencyUs / snapshot.callCount / 1000).toFixed(2)} ms` : '–'}
                              </td>
                              <td class={`px-4 py-2 text-right ${snapshot.errorCount > 0 ? 'text-error' : ''}`}>
                                {snapshot.errorCount}
                              </td>
                            </tr>
                          )}
                        </For>
                      </tbody>
                    </table>
                  </div>
                </Show>
              </Card>
            );
          }}
        </Show>
      </Show>
    </div>
  );
}
