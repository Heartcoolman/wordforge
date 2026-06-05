import { createSignal, createMemo, Show, For, onMount } from 'solid-js';
import { Card } from '@/components/ui/Card';
import { PageHeader } from '@/components/ui/PageHeader';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Tabs } from '@/components/ui/Tabs';
import { Badge } from '@/components/ui/Badge';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { uiStore } from '@/stores/ui';
import { amasApi } from '@/api/amas';
import { adminApi } from '@/api/admin';
import type { AmasConfig } from '@/types/amas';
import { TierAPanel } from './amas/TierAPanel';
import { SectionPanel } from './amas/SectionPanel';
import { JsonAdvancedPanel } from './amas/JsonAdvancedPanel';
import { PresetBar } from '@/components/amas/PresetBar';
import { AmasVersionDrawer } from '@/components/admin/AmasVersionDrawer';
import { validateConfig, diffKnown } from './amas/schema';

type TabId = 'tier-a' | 'sections' | 'json';

export default function AmasConfigPage() {
  // source-of-truth:宽松对象,承载 ~295 个参数全量
  const [config, setConfig] = createSignal<Record<string, unknown>>({});
  // baseline:从后端拉来的,用于判断"是否有未保存的修改"+ DiffSummary
  const [baseline, setBaseline] = createSignal<Record<string, unknown>>({});
  const [metrics, setMetrics] = createSignal<unknown>(null);
  const [loading, setLoading] = createSignal(true);
  const [loadError, setLoadError] = createSignal<string>('');
  const [saving, setSaving] = createSignal(false);
  const [reloading, setReloading] = createSignal(false);
  const [tab, setTab] = createSignal<TabId>('tier-a');
  const [versionDrawerOpen, setVersionDrawerOpen] = createSignal(false);
  const [showDirtyDrawerConfirm, setShowDirtyDrawerConfirm] = createSignal(false);
  const [showSaveConfirm, setShowSaveConfirm] = createSignal(false);
  const [showReloadConfirm, setShowReloadConfirm] = createSignal(false);
  const [showRollbackConfirm, setShowRollbackConfirm] = createSignal(false);
  // 用于驱动 InlineVersionList refetch
  const [versionRefreshKey, setVersionRefreshKey] = createSignal(0);

  onMount(async () => {
    const [c, m] = await Promise.allSettled([amasApi.getConfig(), amasApi.getMetrics()]);
    if (c.status === 'fulfilled') {
      const cfg = c.value as unknown as Record<string, unknown>;
      setBaseline(structuredClone(cfg));
      setConfig(structuredClone(cfg));
    } else {
      const msg = c.reason instanceof Error ? c.reason.message : '无法获取 AMAS 配置';
      setLoadError(msg);
      uiStore.toast.error('加载失败', msg);
    }
    if (m.status === 'fulfilled') setMetrics(m.value);
    setLoading(false);
  });

  function openVersionDrawer() {
    if (dirty()) {
      setShowDirtyDrawerConfirm(true);
      return;
    }
    setVersionDrawerOpen(true);
  }

  const errors = createMemo(() => validateConfig(config()));
  // 页面级 dirty 用整对象深比较:TomlEditor 可改全量 ~295 参数,diffKnown 仅覆盖
  // PARAM_DICT 收录字段,改字典外字段时 diffKnown 恒 0 会让"验证并应用"锁死无法保存。
  // (深比较对 key 重排会偏 dirty=true,属安全方向:至多允许保存等价配置,绝不漏判。)
  const dirty = createMemo(() => JSON.stringify(baseline()) !== JSON.stringify(config()));
  // dirtyCount 仅用于 DiffSummary 展示"已知参数"改动数,保留 diffKnown 口径。
  const dirtyCount = createMemo(() => diffKnown(baseline(), config()).length);

  function requestSave() {
    const errs = errors();
    if (errs.length > 0) {
      uiStore.toast.error('校验未通过', `${errs.length} 个字段需要修正`);
      return;
    }
    setShowSaveConfirm(true);
  }

  function requestReload() {
    const errs = errors();
    if (errs.length > 0) {
      uiStore.toast.error('校验未通过', `${errs.length} 个字段需要修正`);
      return;
    }
    setShowReloadConfirm(true);
  }

  function requestRollback() {
    setShowRollbackConfirm(true);
  }

  async function saveConfig() {
    setShowSaveConfirm(false);
    try {
      setSaving(true);
      await amasApi.updateConfig(config() as unknown as AmasConfig);
      setBaseline(structuredClone(config()));
      setVersionRefreshKey((k) => k + 1);
      uiStore.toast.success('AMAS 配置已更新');
    } catch (err: unknown) {
      uiStore.toast.error('保存失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setSaving(false);
    }
  }

  async function reloadAmasConfig() {
    setShowReloadConfirm(false);
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

  function rollbackToBaseline() {
    setShowRollbackConfirm(false);
    setConfig(structuredClone(baseline()));
    uiStore.toast.success('已回滚到 baseline');
  }

  return (
    <div class="space-y-4">
      {/* page-header(对齐 amas-config.html .page-header) */}
      <PageHeader
        title="AMAS 调参"
        desc="8 个子配置(~295 参数)· 8 算法 flag(4 决策 + 4 记忆)· ELO + 疲劳信号融合。保存即触发 500ms 防抖 validate → 原子热加载,无需重启。"
        actions={
          <>
            <Show when={errors().length > 0}>
              <Badge variant="error" size="sm">
                {errors().length} 处校验错误
              </Badge>
            </Show>
            <Show when={dirty() && errors().length === 0}>
              <Badge variant="warning" size="sm">
                未保存 · {dirtyCount()} 处
              </Badge>
            </Show>
            <Button size="sm" variant="ghost" onClick={openVersionDrawer}>
              版本历史
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={requestRollback}
              disabled={!dirty()}
              class="hover:bg-warning-light hover:text-warning-strong"
              title={!dirty() ? '无未保存修改' : '丢弃所有未保存修改'}
            >
              回滚到 baseline
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={requestReload}
              loading={reloading()}
              disabled={errors().length > 0 || saving() || !dirty()}
            >
              热重载
            </Button>
            <Button
              size="sm"
              onClick={requestSave}
              loading={saving()}
              disabled={errors().length > 0 || !dirty() || reloading()}
            >
              验证并应用
            </Button>
          </>
        }
      />

      <Show
        when={!loading()}
        fallback={
          <div class="flex justify-center py-12">
            <Spinner size="lg" />
          </div>
        }
      >
        <Show
          when={!loadError()}
          fallback={
            <Card variant="elevated">
              <div class="flex flex-col items-center justify-center py-12 text-center gap-3">
                <p class="text-headline text-content">加载失败,请重试</p>
                <p class="text-sm text-content-secondary">{loadError()}</p>
                <Button size="sm" variant="outline" onClick={() => window.location.reload()}>
                  刷新页面
                </Button>
              </div>
            </Card>
          }
        >
          {/* preset bar(对齐 amas-config.html .preset-bar) */}
          <PresetBar config={config()} baseline={baseline()} onApply={(next) => setConfig(next)} />

          {/* 3 sub tabs(删了"并排视图") */}
          <Tabs
            tabs={[
              { id: 'tier-a', label: '重点参数 (Tier-A · 11 维)' },
              { id: 'sections', label: '分节配置' },
              { id: 'json', label: 'JSON 高级' },
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
            <JsonAdvancedPanel
              config={config()}
              baseline={baseline()}
              errors={errors()}
              onChange={setConfig}
              onOpenVersionDrawer={openVersionDrawer}
              versionRefreshKey={versionRefreshKey}
            />
          </Show>

          <AmasVersionDrawer
            open={versionDrawerOpen()}
            onClose={() => setVersionDrawerOpen(false)}
            currentConfig={config()}
            onRestored={(next) => {
              setBaseline(structuredClone(next));
              setConfig(structuredClone(next));
              setVersionRefreshKey((k) => k + 1);
            }}
          />

          {/* 保存确认:写入新版本,对所有在线用户生效 */}
          <ConfirmDialog
            open={showSaveConfirm()}
            title="确认保存 AMAS 配置"
            message={
              <>
                将保存 <span class="font-medium text-content tabular-nums">{dirtyCount()}</span>{' '}
                处参数修改并写入新版本。
                <br />
                新配置对所有在线用户立即生效,回滚需在「版本历史」中手动操作。
              </>
            }
            confirmText="确认保存"
            variant="warning"
            onConfirm={saveConfig}
            onCancel={() => setShowSaveConfirm(false)}
          />

          {/* 热重载确认:跳过版本化直接覆盖 */}
          <ConfirmDialog
            open={showReloadConfirm()}
            title="确认热重载 AMAS 配置"
            message={
              <>
                将立即把当前编辑值推送到运行时(绕过版本化),所有在线请求下一次调用时使用新参数。
                <br />
                此操作不会生成版本快照,仅用于排错与即时调参;务必先确认无误。
              </>
            }
            confirmText="确认热重载"
            variant="danger"
            onConfirm={reloadAmasConfig}
            onCancel={() => setShowReloadConfirm(false)}
          />

          {/* 回滚 baseline 确认 */}
          <ConfirmDialog
            open={showRollbackConfirm()}
            title="回滚到 baseline"
            message={
              <>
                将丢弃 <span class="font-medium text-content tabular-nums">{dirtyCount()}</span>{' '}
                处未保存修改,恢复到服务端 baseline。此操作不可撤销。
              </>
            }
            confirmText="确认回滚"
            variant="warning"
            onConfirm={rollbackToBaseline}
            onCancel={() => setShowRollbackConfirm(false)}
          />

          {/* 打开版本历史前,提示未保存修改会丢失 */}
          <ConfirmDialog
            open={showDirtyDrawerConfirm()}
            title="存在未保存的修改"
            message="打开版本历史后,未保存修改将无法恢复,确认继续?"
            confirmText="继续打开"
            variant="warning"
            onConfirm={() => {
              setShowDirtyDrawerConfirm(false);
              setVersionDrawerOpen(true);
            }}
            onCancel={() => setShowDirtyDrawerConfirm(false)}
          />

          {/* 算法指标(保留底部,作为运行时观察) */}
          <Show when={metrics()}>
            {(m) => {
              const entries = () =>
                Object.entries(
                  m() as Record<
                    string,
                    { callCount: number; totalLatencyUs: number; errorCount: number }
                  >,
                );
              return (
                <Card variant="elevated">
                  <h2 class="text-headline text-content mb-3">算法指标</h2>
                  <Show
                    when={entries().length > 0}
                    fallback={<p class="text-sm text-content-secondary">暂无指标数据</p>}
                  >
                    <div class="overflow-x-auto">
                      <table class="w-full text-sm">
                        <thead>
                          <tr class="bg-surface-secondary/60 backdrop-blur-sm border-b border-border-hairline">
                            <th
                              scope="col"
                              class="px-4 py-2 text-left font-medium text-content-secondary"
                            >
                              算法名称
                            </th>
                            <th
                              scope="col"
                              class="px-4 py-2 text-right font-medium text-content-secondary"
                            >
                              调用次数
                            </th>
                            <th
                              scope="col"
                              class="px-4 py-2 text-right font-medium text-content-secondary"
                            >
                              平均延迟
                            </th>
                            <th
                              scope="col"
                              class="px-4 py-2 text-right font-medium text-content-secondary"
                            >
                              错误次数
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          <For each={entries()}>
                            {([name, snapshot]) => (
                              <tr class="border-b border-border-hairline hover:bg-accent-light/40 transition-colors duration-150 ease-out">
                                <td class="px-4 py-2 font-mono text-sm">{name}</td>
                                <td class="px-4 py-2 text-right">{snapshot.callCount}</td>
                                <td class="px-4 py-2 text-right">
                                  {snapshot.callCount > 0
                                    ? `${(snapshot.totalLatencyUs / snapshot.callCount / 1000).toFixed(2)} ms`
                                    : '–'}
                                </td>
                                <td
                                  class={`px-4 py-2 text-right ${snapshot.errorCount > 0 ? 'text-error' : ''}`}
                                >
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
      </Show>
    </div>
  );
}
