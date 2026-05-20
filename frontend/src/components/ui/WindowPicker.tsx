import { For } from 'solid-js';
import { cn } from '@/utils/cn';
import { useIndicatorTrack } from '@/lib/motion';

interface WindowPickerProps {
  value: number;
  onChange: (v: number) => void;
  options?: number[];
  /** radiogroup 的 aria-label，默认"时间范围选择" */
  'aria-label'?: string;
}

export function WindowPicker(props: WindowPickerProps) {
  const opts = () => props.options ?? [7, 14, 30];
  let containerRef!: HTMLDivElement;

  const indicator = useIndicatorTrack(
    () => containerRef,
    () => props.value,
    '[data-active="true"]',
  );

  // 读取容器实际 padding-left，避免硬编码 -4px 在不同 padding 设定下错位
  const indicatorOffset = () => {
    const r = containerRef;
    if (!r) return 0;
    const pad = parseFloat(getComputedStyle(r).paddingLeft || '0');
    return indicator().left - pad;
  };

  function handleKeyDown(e: KeyboardEvent) {
    const list = opts();
    const idx = list.indexOf(props.value);
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
      e.preventDefault();
      const next = idx < 0 ? 0 : (idx + 1) % list.length;
      props.onChange(list[next]);
      (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="radio"]')[next]?.focus();
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      e.preventDefault();
      const prev = idx < 0 ? list.length - 1 : (idx - 1 + list.length) % list.length;
      props.onChange(list[prev]);
      (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="radio"]')[prev]?.focus();
    } else if (e.key === 'Home') {
      e.preventDefault();
      props.onChange(list[0]);
      (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="radio"]')[0]?.focus();
    } else if (e.key === 'End') {
      e.preventDefault();
      const last = list.length - 1;
      props.onChange(list[last]);
      (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="radio"]')[last]?.focus();
    }
  }

  return (
    <div
      ref={containerRef}
      role="radiogroup"
      aria-label={props['aria-label'] ?? '时间范围选择'}
      onKeyDown={handleKeyDown}
      class="relative inline-flex items-center p-1 bg-surface-secondary border border-border-hairline rounded-full"
    >
      {/* Indicator — 滑动跟随激活选项；首帧 width=0 时隐藏避免入场动画 */}
      <div
        aria-hidden="true"
        class="absolute h-[calc(100%-0.5rem)] top-1 bg-accent-light shadow-elevation-1 rounded-full pointer-events-none transition-[transform,width] duration-base ease-out-expo"
        style={{
          transform: `translateX(${indicatorOffset()}px)`,
          width: `${indicator().width}px`,
          visibility: indicator().width === 0 ? 'hidden' : 'visible',
        }}
      />
      <For each={opts()}>
        {(days) => {
          const active = () => props.value === days;
          return (
            <button
              type="button"
              role="radio"
              aria-checked={active()}
              tabIndex={active() ? 0 : -1}
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
