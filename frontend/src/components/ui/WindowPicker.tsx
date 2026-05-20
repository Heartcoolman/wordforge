import { For } from 'solid-js';
import { cn } from '@/utils/cn';
import { useIndicatorTrack } from '@/lib/motion';

interface WindowPickerProps {
  value: () => number;
  onChange: (v: number) => void;
  options?: number[];
}

export function WindowPicker(props: WindowPickerProps) {
  const opts = () => props.options ?? [7, 14, 30];
  let containerRef: HTMLDivElement | undefined;

  const indicator = useIndicatorTrack(
    () => containerRef,
    () => props.value(),
    '[data-active="true"]',
  );

  return (
    <div
      ref={containerRef}
      role="group"
      aria-label="时间范围选择"
      class="relative inline-flex items-center p-1 bg-surface-secondary border border-border-hairline rounded-full"
    >
      {/* Indicator — 滑动跟随激活选项 */}
      <div
        aria-hidden="true"
        class="absolute h-[calc(100%-0.5rem)] top-1 bg-accent-light shadow-elevation-1 rounded-full pointer-events-none transition-[transform,width] duration-base ease-out-expo"
        style={{
          transform: `translateX(${indicator().left - 4}px)`,
          width: `${indicator().width}px`,
        }}
      />
      <For each={opts()}>
        {(days) => {
          const active = () => props.value() === days;
          return (
            <button
              type="button"
              aria-pressed={active()}
              data-active={active() ? 'true' : undefined}
              onClick={() => props.onChange(days)}
              class={cn(
                'relative px-4 py-1.5 text-sm font-medium rounded-full cursor-pointer outline-none',
                'transition-colors duration-fast ease-out-expo',
                'focus-visible:ring-2 focus-visible:ring-accent',
                active() ? 'text-accent' : 'text-content-secondary hover:text-content',
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
