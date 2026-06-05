import { Show, For, createResource } from 'solid-js';
import { adminApi, type AmasConfigVersionSource } from '@/api/admin';
import { Spinner } from '@/components/ui/Spinner';
import { Card } from '@/components/ui/Card';
import { cn } from '@/utils/cn';

interface InlineVersionListProps {
  /** 点击 header "查看全部" 触发,父打开 AmasVersionDrawer */
  onOpenFullList: () => void;
  /** 刷新 trigger;父在 save 后 toggle 此值,触发 refetch */
  refreshKey?: () => unknown;
}

const SOURCE_LABEL: Record<AmasConfigVersionSource, string> = {
  manual: '人工',
  llm_suggested: 'LLM 建议',
  llm_auto: 'LLM 自动',
};

/**
 * 版本列表(JSON pane 右栏内嵌精简版,对齐 amas-config.html .panel "历史版本"):
 * - 显示最近 10 个版本(latest 高亮 accent-light 底)
 * - 每行: version hash + 相对时间 + note + source
 * - header 点击"最近 10 次"触发 onOpenFullList 打开完整 Drawer
 *
 * 与 AmasVersionDrawer 区别:
 * - Drawer: 全模态 / 50 条 / 可展开看 diff / 可回滚
 * - InlineVersionList: 卡片内嵌 / 10 条 / 仅展示 / 跳转到 Drawer 做操作
 */
export function InlineVersionList(props: InlineVersionListProps) {
  const [versions] = createResource(
    () => props.refreshKey?.() ?? 'init',
    async () => await adminApi.amasListVersions(10),
  );

  return (
    <Card variant="outlined" padding="none">
      <div class="flex items-baseline justify-between px-4 py-3 border-b border-border-hairline">
        <h3 class="text-sm font-semibold text-content">历史版本</h3>
        <button
          type="button"
          class="focus-ring-soft text-[11px] font-mono text-content-tertiary hover:text-accent transition-colors rounded px-1 -mx-1"
          onClick={props.onOpenFullList}
        >
          最近 10 次 →
        </button>
      </div>
      <Show
        when={!versions.loading}
        fallback={
          <div class="flex justify-center py-6">
            <Spinner size="sm" />
          </div>
        }
      >
        <Show
          when={(versions() ?? []).length > 0}
          fallback={
            <p class="px-4 py-4 text-center text-xs text-content-tertiary">
              尚无历史版本
            </p>
          }
        >
          <ul class="divide-y divide-border-hairline max-h-[320px] overflow-y-auto">
            <For each={versions() ?? []}>
              {(v, idx) => (
                <li
                  class={cn(
                    'px-4 py-2.5 transition-colors',
                    idx() === 0 && 'bg-accent-light/40',
                  )}
                >
                  <div class="flex items-baseline justify-between gap-2">
                    <code
                      class={cn(
                        'font-mono text-[12px] tabular-nums',
                        idx() === 0 ? 'text-accent font-semibold' : 'text-content',
                      )}
                    >
                      v{v.versionHash.slice(0, 6)}
                      <Show when={idx() === 0}>
                        <span class="ml-1 text-[10px] font-normal">· 当前</span>
                      </Show>
                    </code>
                    <span class="text-[10px] font-mono text-content-tertiary tabular-nums">
                      {relTime(v.createdAt)}
                    </span>
                  </div>
                  <div class="mt-1 text-[11px] text-content-secondary leading-snug">
                    <Show
                      when={v.note}
                      fallback={
                        <span class="italic text-content-tertiary">(无备注)</span>
                      }
                    >
                      <span class="line-clamp-1">{v.note}</span>
                    </Show>
                    <span class="ml-1 text-content-tertiary">
                      · {SOURCE_LABEL[v.source]}
                    </span>
                  </div>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>
    </Card>
  );
}

function relTime(iso: string): string {
  try {
    const t = new Date(iso).getTime();
    const diff = Date.now() - t;
    if (diff < 60_000) return '刚刚';
    if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m`;
    if (diff < 86400_000) return `${Math.floor(diff / 3600_000)}h`;
    return `${Math.floor(diff / 86400_000)}d`;
  } catch {
    return iso;
  }
}
