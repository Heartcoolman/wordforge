import { For, Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface PaginationProps {
  page: number;
  total: number;
  pageSize: number;
  onChange: (page: number) => void;
  class?: string;
}

export function Pagination(props: PaginationProps) {
  const totalPages = () => Math.max(1, Math.ceil(props.total / props.pageSize));
  const hasPrev = () => props.page > 1;
  const hasNext = () => props.page < totalPages();

  const pages = () => {
    const t = totalPages();
    const c = props.page;
    const items: (number | '...')[] = [];
    if (t <= 7) {
      for (let i = 1; i <= t; i++) items.push(i);
    } else {
      items.push(1);
      if (c > 3) items.push('...');
      for (let i = Math.max(2, c - 1); i <= Math.min(t - 1, c + 1); i++) items.push(i);
      if (c < t - 2) items.push('...');
      items.push(t);
    }
    return items;
  };

  return (
    <Show when={totalPages() > 1}>
      <nav aria-label="分页" class={cn('flex items-center gap-1', props.class)}>
        <button
          disabled={!hasPrev()}
          onClick={() => props.onChange(props.page - 1)}
          aria-label="上一页"
          class="p-2 rounded-lg text-content-secondary hover:bg-surface-secondary disabled:opacity-40 disabled:pointer-events-none transition-colors cursor-pointer"
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
                  'w-8 h-8 rounded-lg text-sm font-medium transition-colors cursor-pointer',
                  props.page === item
                    ? 'bg-accent text-accent-content'
                    : 'text-content-secondary hover:bg-surface-secondary',
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
          class="p-2 rounded-lg text-content-secondary hover:bg-surface-secondary disabled:opacity-40 disabled:pointer-events-none transition-colors cursor-pointer"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
            <path stroke-linecap="round" stroke-linejoin="round" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </nav>
    </Show>
  );
}
