import { For, Show, createMemo } from 'solid-js';
import { cn } from '@/utils/cn';

interface PaginationProps {
  page: number;
  total: number;
  pageSize: number;
  onChange: (page: number) => void;
  class?: string;
}

export function Pagination(props: PaginationProps) {
  // pageSize=0 触发 Math.ceil(total/0)=Infinity，用 max(1) 兜底
  const totalPages = createMemo(() =>
    Math.max(1, Math.ceil(props.total / Math.max(1, props.pageSize))),
  );
  const hasPrev = () => props.page > 1;
  const hasNext = () => props.page < totalPages();

  // 修正：原实现在 c<=3 时把 [1,2,3,...,t] 中的 2/3 漏掉（Math.max(2, c-1)=2 起始，
  // 但 c<=3 时未生成 leading 省略，结果只有 [1, t]）。
  // 改用统一窗口：start = max(2, c-1)，end = min(t-1, c+1)，再决定是否插入省略号。
  const pages = createMemo(() => {
    const t = totalPages();
    const c = props.page;
    if (t <= 7) return Array.from({ length: t }, (_, i) => i + 1) as (number | '...')[];
    const res: (number | '...')[] = [1];
    const start = Math.max(2, c - 1);
    const end = Math.min(t - 1, c + 1);
    if (start > 2) res.push('...');
    for (let i = start; i <= end; i++) res.push(i);
    if (end < t - 1) res.push('...');
    res.push(t);
    return res;
  });

  return (
    <Show
      when={totalPages() > 1}
      fallback={
        // total=0/1 时占位维持布局高度，避免周边元素抖动
        <div class={cn('flex items-center h-9', props.class)} aria-hidden="true" />
      }
    >
      <nav aria-label="分页" class={cn('flex items-center gap-1', props.class)}>
        <button
          disabled={!hasPrev()}
          onClick={() => props.onChange(props.page - 1)}
          aria-label="上一页"
          class="p-2 rounded-lg text-content-secondary hover:bg-surface-secondary hover:text-content disabled:opacity-40 transition-colors duration-fast cursor-pointer disabled:cursor-not-allowed"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <For each={pages()}>
          {(item) =>
            item === '...' ? (
              <span class="px-2 text-content-tertiary">...</span>
            ) : (
              <button
                onClick={() => props.onChange(item as number)}
                aria-label={`第 ${item} 页`}
                aria-current={props.page === item ? 'page' : undefined}
                class={cn(
                  'w-8 h-8 rounded-lg text-sm font-medium cursor-pointer',
                  'transition-[background-color,color,box-shadow,transform] duration-fast ease-out-expo',
                  props.page === item
                    ? 'bg-accent text-accent-content shadow-elevation-1'
                    : 'text-content-secondary hover:bg-surface-secondary hover:text-content',
                )}
              >
                {item}
              </button>
            )
          }
        </For>
        <button
          disabled={!hasNext()}
          onClick={() => props.onChange(props.page + 1)}
          aria-label="下一页"
          class="p-2 rounded-lg text-content-secondary hover:bg-surface-secondary hover:text-content disabled:opacity-40 transition-colors duration-fast cursor-pointer disabled:cursor-not-allowed"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </nav>
    </Show>
  );
}
