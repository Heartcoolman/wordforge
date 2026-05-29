import { createResource, createSignal, For, Show } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { ConfirmDialog } from '@/components/ui/ConfirmDialog';
import { adminApi, type WhitelistRow } from '@/api/admin';
import { uiStore } from '@/stores/ui';

export function WhitelistPanel() {
  const [rows, { refetch }] = createResource(() => adminApi.amasListWhitelist());
  const [path, setPath] = createSignal('');
  const [minSafe, setMinSafe] = createSignal('');
  const [maxSafe, setMaxSafe] = createSignal('');
  const [adding, setAdding] = createSignal(false);
  const [delTarget, setDelTarget] = createSignal<WhitelistRow | null>(null);
  const [deleting, setDeleting] = createSignal(false);

  async function add() {
    const p = path().trim();
    const lo = parseFloat(minSafe());
    const hi = parseFloat(maxSafe());
    if (!p || !Number.isFinite(lo) || !Number.isFinite(hi)) {
      uiStore.toast.error('请填写合法 path 与区间');
      return;
    }
    if (lo > hi) {
      uiStore.toast.error('minSafe 不能大于 maxSafe');
      return;
    }
    setAdding(true);
    try {
      await adminApi.amasAddWhitelist({ path: p, minSafe: lo, maxSafe: hi });
      uiStore.toast.success('已添加白名单条目');
      setPath(''); setMinSafe(''); setMaxSafe('');
      void refetch();
    } catch (e) {
      uiStore.toast.error('添加失败', e instanceof Error ? e.message : '');
    } finally {
      setAdding(false);
    }
  }

  async function confirmDelete() {
    const t = delTarget();
    if (!t) return;
    setDeleting(true);
    try {
      await adminApi.amasDeleteWhitelist(t.path);
      uiStore.toast.success('已删除');
      setDelTarget(null);
      void refetch();
    } catch (e) {
      uiStore.toast.error('删除失败', e instanceof Error ? e.message : '');
    } finally {
      setDeleting(false);
    }
  }

  const inputCls = 'h-9 px-2 rounded-lg text-sm bg-surface text-content border border-border-hairline focus-ring-soft focus:border-accent';

  return (
    <div class="panel">
      <div class="panel-title">调参白名单</div>
      <Show when={!rows.error} fallback={<Empty title="白名单加载失败" description={rows.error instanceof Error ? rows.error.message : ''} />}>
        <Show when={rows()} fallback={<div class="flex justify-center py-8"><Spinner size="sm" /></div>}>
          <Show when={(rows() ?? []).length > 0} fallback={<Empty title="白名单为空" description="启动 seed 应填充 TIER_A_WHITELIST" />}>
            <div class="whitelist-grid mb-4">
              <For each={rows() ?? []}>
                {(r) => (
                  <div class="item">
                    <span class="led" aria-hidden="true" />
                    <span class="name truncate" title={r.path}>{r.path}</span>
                    <span class="range tabular-nums">[{r.minSafe}, {r.maxSafe}]</span>
                    <button
                      type="button"
                      aria-label={`删除 ${r.path}`}
                      onClick={() => setDelTarget(r)}
                      class="text-content-tertiary hover:text-error transition-colors"
                    >
                      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg>
                    </button>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </Show>
      </Show>

      <div class="border-t border-border-hairline pt-3 grid grid-cols-[1fr_auto_auto_auto] gap-2 items-end">
        <input class={inputCls} placeholder="memoryModel.xxx" value={path()} onInput={(e) => setPath(e.currentTarget.value)} />
        <input class={`${inputCls} w-20`} type="number" step="0.01" aria-label="min" placeholder="min" value={minSafe()} onInput={(e) => setMinSafe(e.currentTarget.value)} />
        <input class={`${inputCls} w-20`} type="number" step="0.01" aria-label="max" placeholder="max" value={maxSafe()} onInput={(e) => setMaxSafe(e.currentTarget.value)} />
        <Button size="sm" loading={adding()} onClick={add}>添加</Button>
      </div>

      <ConfirmDialog
        open={!!delTarget()}
        title="确认删除白名单条目"
        message={<>将移除 <span class="font-mono">{delTarget()?.path}</span>，advisor 后续 patch 将拒绝该参数。</>}
        confirmText="确认删除"
        variant="danger"
        loading={deleting()}
        onConfirm={confirmDelete}
        onCancel={() => setDelTarget(null)}
      />
    </div>
  );
}
