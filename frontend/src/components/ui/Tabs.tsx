import { createSignal, For, type JSX } from 'solid-js';
import { cn } from '@/utils/cn';
import { useIndicatorTrack } from '@/lib/motion';

interface Tab {
  id: string;
  label: string;
  icon?: JSX.Element;
}

interface TabsProps {
  tabs: Tab[];
  active: string;
  onChange: (id: string) => void;
  class?: string;
}

export function Tabs(props: TabsProps) {
  const [focusedIndex, setFocusedIndex] = createSignal(-1);
  let containerRef: HTMLDivElement | undefined;

  // 测量激活 tab 的位置，驱动底部 indicator 滑动
  const indicator = useIndicatorTrack(
    () => containerRef,
    () => props.active,
    '[data-active="true"]',
  );

  function handleKeyDown(e: KeyboardEvent) {
    const tabs = props.tabs;
    const currentIndex = focusedIndex() >= 0 ? focusedIndex() : tabs.findIndex((t) => t.id === props.active);

    if (e.key === 'ArrowRight') {
      e.preventDefault();
      const next = (currentIndex + 1) % tabs.length;
      setFocusedIndex(next);
      props.onChange(tabs[next].id);
      (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="tab"]')[next]?.focus();
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      const prev = (currentIndex - 1 + tabs.length) % tabs.length;
      setFocusedIndex(prev);
      props.onChange(tabs[prev].id);
      (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="tab"]')[prev]?.focus();
    }
  }

  return (
    <div
      ref={containerRef}
      class={cn('relative flex border-b border-border-hairline', props.class)}
      role="tablist"
      onKeyDown={handleKeyDown}
    >
      <For each={props.tabs}>
        {(tab) => {
          const isActive = () => props.active === tab.id;
          return (
            <button
              role="tab"
              aria-selected={isActive()}
              tabIndex={isActive() ? 0 : -1}
              data-active={isActive() ? 'true' : undefined}
              onClick={() => props.onChange(tab.id)}
              class={cn(
                'flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium cursor-pointer',
                'transition-[color] duration-fast ease-out-expo',
                isActive()
                  ? 'text-accent'
                  : 'text-content-secondary hover:text-content',
              )}
            >
              {tab.icon}
              {tab.label}
            </button>
          );
        }}
      </For>
      {/* Indicator — 通过 transform 滑动，复用 CSS transition 实现平滑跟随 */}
      <div
        aria-hidden="true"
        class="absolute bottom-0 h-0.5 bg-accent rounded-full pointer-events-none transition-[transform,width] duration-base ease-out-expo"
        style={{
          transform: `translateX(${indicator().left}px)`,
          width: `${indicator().width}px`,
        }}
      />
    </div>
  );
}
