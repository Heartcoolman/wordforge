import { createSignal, For, Show, createResource } from 'solid-js';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Spinner } from '@/components/ui/Spinner';
import { Empty } from '@/components/ui/Empty';
import { adminApi, type AmasConfigVersionRow, type AmasConfigVersionSource } from '@/api/admin';
import { uiStore } from '@/stores/ui';
import { diffKnown } from '@/pages/admin/amas/schema';

interface AmasVersionDrawerProps {
  open: boolean;
  onClose: () => void;
  currentConfig: Record<string, unknown>;
  /** 回滚成功后调用，让父页面拉取最新 config / 版本时间线 */
  onRestored: (newConfig: Record<string, unknown>) => void;
}

const SOURCE_LABEL: Record<AmasConfigVersionSource, string> = {
  manual: '人工',
  llm_suggested: 'LLM 建议',
  llm_auto: 'LLM 自动',
};

const SOURCE_VARIANT: Record<AmasConfigVersionSource, 'default' | 'info' | 'warning'> = {
  manual: 'default',
  llm_suggested: 'info',
  llm_auto: 'warning',
};

export function AmasVersionDrawer(props: AmasVersionDrawerProps) {
  const [expanded, setExpanded] = createSignal<string | null>(null);
  const [confirming, setConfirming] = createSignal<{ hash: string; label: string } | null>(null);
  const [restoring, setRestoring] = createSignal(false);

  // 拉取版本列表；open 切换为 true 时自动获取
  const [versions, { refetch }] = createResource(
    () => props.open,
    async (open) => (open ? await adminApi.amasListVersions(50) : []),
  );

  // 展开某版本：拉详情，做 diff
  const [detail] = createResource(expanded, async (hash) => {
    if (!hash) return null;
    return await adminApi.amasGetVersion(hash);
  });

  const diff = () => {
    const d = detail();
    if (!d) return [];
    return diffKnown(props.currentConfig, d.snapshotJson);
  };

  async function performRestore(hash: string) {
    setRestoring(true);
    try {
      await adminApi.amasRestoreVersion(hash);
      // 拉一次新配置回填到父组件
      const latest = await import('@/api/amas').then((m) => m.amasApi.getConfig());
      props.onRestored(latest as unknown as Record<string, unknown>);
      uiStore.toast.success('已回滚', '配置已切换至所选版本');
      setConfirming(null);
      void refetch();
    } catch (err) {
      uiStore.toast.error('回滚失败', err instanceof Error ? err.message : '未知错误');
    } finally {
      setRestoring(false);
    }
  }

  return (
    <Show when={props.open}>
      <div class="fixed inset-0 z-40 flex justify-end animate-fade-in" onClick={props.onClose}>
        <div
          class="bg-surface-elevated w-full max-w-md h-full shadow-2xl overflow-hidden flex flex-col"
          onClick={(e) => e.stopPropagation()}
        >
          <div class="px-5 py-4 border-b border-border flex items-baseline justify-between">
            <div>
              <h3 class="text-base font-semibold text-content">版本历史</h3>
              <p class="text-xs text-content-tertiary mt-0.5">最近 50 个变更；点击展开可查看 diff 与回滚</p>
            </div>
            <button type="button" class="text-content-tertiary hover:text-content text-xl" onClick={props.onClose}>×</button>
          </div>

          <div class="flex-1 overflow-y-auto px-3 py-3 space-y-2">
            <Show when={!versions.loading} fallback={<div class="flex justify-center py-12"><Spinner /></div>}>
              <Show when={(versions() ?? []).length > 0} fallback={<Empty title="尚无版本" description="保存或回滚后会自动生成第一个版本" />}>
                <For each={versions() ?? []}>
                  {(v, idx) => (
                    <VersionItem
                      v={v}
                      isLatest={idx() === 0}
                      expanded={expanded() === v.versionHash}
                      onToggle={() => setExpanded(expanded() === v.versionHash ? null : v.versionHash)}
                      detailLoading={expanded() === v.versionHash && detail.loading}
                      diff={expanded() === v.versionHash ? diff() : []}
                      onRestore={() => setConfirming({ hash: v.versionHash, label: shortHash(v.versionHash) })}
                    />
                  )}
                </For>
              </Show>
            </Show>
          </div>
        </div>
      </div>

      <Show when={confirming()}>
        {(c) => (
          <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setConfirming(null)}>
            <div class="bg-surface-elevated rounded-xl shadow-2xl max-w-md w-full mx-4" onClick={(e) => e.stopPropagation()}>
              <div class="px-5 py-4 border-b border-border">
                <h3 class="text-base font-semibold text-content">回滚至版本 {c().label}</h3>
              </div>
              <div class="px-5 py-4 text-sm text-content-secondary leading-relaxed space-y-2">
                <p>该操作将：</p>
                <ul class="list-disc pl-5 space-y-1">
                  <li>把运行配置切换为所选版本的快照</li>
                  <li>写回 <code class="font-mono text-xs">amas_config.toml</code></li>
                  <li>在版本表新增一条 source=manual、note=回滚标记</li>
                </ul>
                <p class="text-content-tertiary text-xs">这是「逻辑回滚」（追加一条新版本），不是物理删除中间版本。</p>
              </div>
              <div class="px-5 py-3 border-t border-border flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setConfirming(null)} disabled={restoring()}>取消</Button>
                <Button size="sm" variant="danger" loading={restoring()} onClick={() => performRestore(c().hash)}>确认回滚</Button>
              </div>
            </div>
          </div>
        )}
      </Show>
    </Show>
  );
}

interface VersionItemProps {
  v: AmasConfigVersionRow;
  isLatest: boolean;
  expanded: boolean;
  detailLoading: boolean;
  diff: { path: string; label_zh: string; before: unknown; after: unknown }[];
  onToggle: () => void;
  onRestore: () => void;
}

function VersionItem(props: VersionItemProps) {
  return (
    <div class="border border-border rounded-lg overflow-hidden">
      <button
        type="button"
        class="w-full flex items-start justify-between px-3 py-2 text-left hover:bg-surface-secondary/40 transition-colors"
        onClick={props.onToggle}
      >
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-1.5 flex-wrap">
            <span class="font-mono text-xs text-content">{shortHash(props.v.versionHash)}</span>
            <Badge variant={SOURCE_VARIANT[props.v.source]} size="sm">{SOURCE_LABEL[props.v.source]}</Badge>
            <Show when={props.isLatest}><Badge variant="success" size="sm">当前</Badge></Show>
          </div>
          <div class="text-xs text-content-tertiary mt-1 truncate">
            {props.v.note ?? <span class="italic">（无备注）</span>}
          </div>
          <div class="text-[10px] text-content-tertiary mt-0.5">
            {formatTime(props.v.createdAt)} · {props.v.authorAdminId}
          </div>
        </div>
        <span class="text-xs text-content-tertiary shrink-0 mt-1">{props.expanded ? '收起' : '展开'}</span>
      </button>

      <Show when={props.expanded}>
        <div class="border-t border-border bg-surface-secondary/30 px-3 py-3 space-y-2">
          <Show when={!props.detailLoading} fallback={<div class="flex justify-center py-4"><Spinner size="sm" /></div>}>
            <div class="flex items-baseline justify-between gap-2">
              <span class="text-xs font-medium text-content-secondary">
                与当前差异（{props.diff.length} 项）
              </span>
              <Show when={!props.isLatest}>
                <Button size="xs" variant="outline" onClick={props.onRestore}>回滚到此版本</Button>
              </Show>
            </div>
            <Show when={props.diff.length > 0} fallback={<p class="text-xs text-content-tertiary py-2 text-center">字典范围内无差异</p>}>
              <table class="w-full text-[11px] font-mono">
                <tbody>
                  <For each={props.diff}>
                    {(d) => (
                      <tr class="border-b border-border/40 last:border-b-0">
                        <td class="py-1 pr-2 text-content-secondary">
                          <div class="text-content text-xs font-sans">{d.label_zh}</div>
                          <div class="text-[10px] text-content-tertiary">{d.path}</div>
                        </td>
                        <td class="py-1 pr-2 text-right text-content-tertiary">{formatVal(d.before)}</td>
                        <td class="py-1 text-right text-success">{formatVal(d.after)}</td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </Show>
          </Show>
        </div>
      </Show>
    </div>
  );
}

function shortHash(h: string): string {
  return h.slice(0, 10);
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString('zh-CN', { hour12: false });
  } catch {
    return iso;
  }
}

function formatVal(v: unknown): string {
  if (typeof v === 'boolean') return v ? 'true' : 'false';
  if (typeof v === 'number') return Number.isInteger(v) ? String(v) : v.toFixed(6).replace(/0+$/, '').replace(/\.$/, '');
  if (v === undefined) return '—';
  return JSON.stringify(v);
}
