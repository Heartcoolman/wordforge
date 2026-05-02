import { For } from 'solid-js';
import { cn } from '@/utils/cn';

interface WindowPickerProps {
  value: () => number;
  onChange: (v: number) => void;
  options?: number[];
}

export function WindowPicker(props: WindowPickerProps) {
  const opts = () => props.options ?? [7, 14, 30];

  return (
    <div
      role="group"
      aria-label="时间范围选择"
      class="inline-flex items-center p-1 bg-surface-secondary border border-border rounded-full"
    >
      <For each={opts()}>
        {(days) => {
          const active = () => props.value() === days;
          return (
            <button
              type="button"
              aria-pressed={active()}
              onClick={() => props.onChange(days)}
              class={cn(
                'px-4 py-1.5 text-sm font-medium rounded-full transition-colors cursor-pointer outline-none',
                'focus-visible:ring-2 focus-visible:ring-accent',
                active()
                  ? 'bg-accent-light text-accent shadow-sm'
                  : 'text-content-secondary hover:text-content',
              )}
            >
              {days}天
            </button>
          );
        }}
      </For>
    </div>
  );
}
