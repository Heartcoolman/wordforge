import { createSignal, createMemo, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Tabs } from '@/components/ui/Tabs';
import { Badge } from '@/components/ui/Badge';
import { uiStore } from '@/stores/ui';
import { amasApi } from '@/api/amas';
import { adminApi } from '@/api/admin';
import type { AmasConfig } from '@/types/amas';
import { TierAPanel } from './amas/TierAPanel';
import { SectionPanel } from './amas/SectionPanel';
import { JsonAdvancedPanel } from './amas/JsonAdvancedPanel';
import { PresetSelector } from './amas/PresetSelector';
import { AmasVersionDrawer } from '@/components/admin/AmasVersionDrawer';
import { validateConfig, diffKnown } from './amas/schema';

type TabId = 'tier-a' | 'sections' | 'json';

export default function AmasConfigPage() {
  // source-of-truth：宽松对象，承载 ~295 个参数全量
  const [config, setConfig] = createSignal<Record<string, unknown>>({});
  // baseline：从后端拉来的、用于判断"是否有未保存的修改"
  const [baseline, setBaseline] = createSignal<Record<string, unknown>>({});
  const [metrics, setMetrics] = createSignal<unknown>(null);
  const [loading, setLoading] = createSignal(true);
  const [saving, setSaving] = createSignal(false);
  const [reloading, setReloading] = createSignal(false);
  const [tab, setTab] = createSignal<TabId>('tier-a');
  const [versionDrawerOpen, setVersionDrawerOpen] = createSignal(false);

  onMount(async () => {
    const [c, m] = await Promise.allSettled([amasApi.getConfig(), amasApi.getMetrics()]);
    if (c.status === 'fulfilled') {
      const cfg = c.value as unknown as Record<string, unknown>;
      setBaseline(structuredClone(cfg));
      setConfig(structuredClone(cfg));
    } else {
      uiStore.toast.error('加载失败', '无法获取 AMAS 配置');
    }
    if (m.status === 'fulfilled') setMetrics(m.value);
    setLoading(false);
  });

  const errors = createMemo(() => validateConfig(config()));
  const dirty = createMemo(() => diffKnown(baseline(), config()).length > 0);

  async function saveConfig() {
    const errs = errors();
    if (errs.length > 0) {
      uiStore.toast.error('校验未通过', `${errs.length} 个字段需要修正`);
      return;
    }
    try {
      setSaving(true);
      await amasApi.updateConfig(config() as unknown as AmasConfig);
      setBaseline(structuredClone(config()));
      uiStore.toast.success('AMAS 配置已更新');
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setSaving(false);
    }
  }

  async function reloadAmasConfig() {
    const errs = errors();
    if (errs.length > 0) {
      uiStore.toast.error('校验未通过', `${errs.length} 个字段需要修正`);
      return;
    }
    try {
      setReloading(true);
      const latest = await adminApi.reloadAmas(config() as unknown as AmasConfig);
      const cfg = latest as unknown as Record<string, unknown>;
      setBaseline(structuredClone(cfg));
      setConfig(structuredClone(cfg));
      uiStore.toast.success('AMAS 配置已热重载');
    } catch (err: unknown) {
      uiStore.toast.error('热重载失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setReloading(false);
    }
  }

  function discardChanges() {
    setConfig(structuredClone(baseline()));
  }

  return (
    <div class="space-y-4 animate-fade-in-up">
      <Show when={!loading()} fallback={<div class="flex justify-center py-12"><Spinner size="lg" /></div>}>
        <Card variant="elevated">
          <div class="flex flex-col gap-3">
            <div class="flex items-baseline justify-between flex-wrap gap-3">
              <div>
                <h2 class="text-lg font-semibold text-content">AMAS 调参</h2>
                <p class="text-xs text-content-tertiary mt-0.5">
                  共 ~295 个参数，先在「重点参数」调 11 维核心、其余在「分节配置」或「JSON 高级」编辑
                </p>
              </div>
              <div class="flex items-center gap-3 flex-wrap">
                <Show when={errors().length > 0}>
                  <Badge variant="error">{errors().length} 处校验错误</Badge>
                </Show>
                <Show when={dirty() && errors().length === 0}>
                  <Badge variant="warning">未保存的修改</Badge>
                </Show>
                <Show when={dirty()}>
                  <Button size="sm" variant="ghost" onClick={discardChanges}>放弃修改</Button>
                </Show>
                <Button size="sm" variant="ghost" onClick={() => setVersionDrawerOpen(true)}>
                  版本历史
                </Button>
                <Button size="sm" variant="outline" onClick={reloadAmasConfig} loading={reloading()} disabled={errors().length > 0}>
                  热重载
                </Button>
                <Button size="sm" onClick={saveConfig} loading={saving()} disabled={errors().length > 0 || !dirty()}>
                  保存配置
                </Button>
              </div>
            </div>

            <PresetSelector config={config()} onApply={(next) => setConfig(next)} />

            <Tabs
              tabs={[
                { id: 'tier-a', label: `重点参数（Tier-A · 11 维）` },
                { id: 'sections', label: `分节配置` },
                { id: 'json', label: `JSON 高级` },
              ]}
              active={tab()}
              onChange={(id) => setTab(id as TabId)}
            />

            <Show when={tab() === 'tier-a'}>
              <TierAPanel config={config()} errors={errors()} onChange={setConfig} />
            </Show>
            <Show when={tab() === 'sections'}>
              <SectionPanel config={config()} errors={errors()} onChange={setConfig} />
            </Show>
            <Show when={tab() === 'json'}>
              <JsonAdvancedPanel config={config()} onChange={setConfig} />
            </Show>
          </div>
        </Card>

        <AmasVersionDrawer
          open={versionDrawerOpen()}
          onClose={() => setVersionDrawerOpen(false)}
          currentConfig={config()}
          onRestored={(next) => {
            setBaseline(structuredClone(next));
            setConfig(structuredClone(next));
          }}
        />

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
                          <th scope="col" class="px-4 py-2 text-left font-medium text-content-secondary">算法名称</th>
                          <th scope="col" class="px-4 py-2 text-right font-medium text-content-secondary">调用次数</th>
                          <th scope="col" class="px-4 py-2 text-right font-medium text-content-secondary">平均延迟</th>
                          <th scope="col" class="px-4 py-2 text-right font-medium text-content-secondary">错误次数</th>
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
