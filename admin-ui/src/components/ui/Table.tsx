import { For, Show, type JSX } from 'solid-js';
import { cn } from '@/utils/cn';

interface Column<T> {
  key: string;
  title: string;
  render?: (row: T, index: number) => JSX.Element;
  class?: string;
}

interface TableProps<T> {
  columns: Column<T>[];
  data: T[];
  loading?: boolean;
  loadingRows?: number;
  emptyText?: string;
  onRowClick?: (row: T) => void;
  class?: string;
  'aria-label'?: string;
  /** sr-only 表格说明（无障碍语义） */
  caption?: string;
}

export function Table<T extends Record<string, unknown>>(props: TableProps<T>) {
  // 行键盘交互：onRowClick 模式下 Enter/Space 触发
  const onRowKeyDown = (row: T) => (e: KeyboardEvent) => {
    if (!props.onRowClick) return;
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      props.onRowClick(row);
    }
  };

  return (
    <div
      class={cn(
        // 窄屏允许横滚兜底，≥md 关闭横滚，避免桌面端"假横滚"违规
        'overflow-x-auto md:overflow-x-visible rounded-xl border border-border-hairline shadow-elevation-1',
        props.class,
      )}
    >
      <table class="w-full text-sm" aria-label={props['aria-label']}>
        <Show when={props.caption}>
          <caption class="sr-only">{props.caption}</caption>
        </Show>
        <thead>
          <tr class="border-b border-border-hairline bg-surface-secondary/60 backdrop-blur-sm">
            <For each={props.columns}>
              {(col) => (
                <th
                  scope="col"
                  class={cn('px-4 py-3 text-left font-medium text-content-secondary text-caption uppercase tracking-wide', col.class)}
                >
                  {col.title}
                </th>
              )}
            </For>
          </tr>
        </thead>
        <tbody class="transition-opacity duration-150">
          <Show when={props.loading}>
            <For each={Array.from({ length: props.loadingRows ?? 3 }, (_, i) => i)}>
              {() => (
                <tr class="border-b border-border-hairline last:border-b-0">
                  <For each={props.columns}>
                    {() => (
                      <td class="px-4 py-3">
                        <div class="h-5 animate-shimmer rounded" />
                      </td>
                    )}
                  </For>
                </tr>
              )}
            </For>
          </Show>
          <Show when={!props.loading && props.data.length === 0}>
            <tr>
              <td colspan={props.columns.length} class="px-4 py-8 text-center text-content-tertiary">
                {props.emptyText ?? '暂无数据'}
              </td>
            </tr>
          </Show>
          <Show when={!props.loading}>
            <For each={props.data}>
              {(row, index) => (
                <tr
                  class={cn(
                    'border-b border-border-hairline last:border-b-0',
                    'transition-colors duration-fast ease-out-expo',
                    props.onRowClick && 'hover:bg-accent-light/40 cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent',
                  )}
                  role={props.onRowClick ? 'button' : undefined}
                  tabIndex={props.onRowClick ? 0 : undefined}
                  onClick={() => props.onRowClick?.(row)}
                  onKeyDown={props.onRowClick ? onRowKeyDown(row) : undefined}
                >
                  <For each={props.columns}>
                    {(col) => (
                      <td class={cn('px-4 py-3 text-content', col.class)}>
                        {col.render ? col.render(row, index()) : String(row[col.key] ?? '')}
                      </td>
                    )}
                  </For>
                </tr>
              )}
            </For>
          </Show>
        </tbody>
      </table>
    </div>
  );
}
