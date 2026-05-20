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
  /**
   * activation: 'auto'（默认，向后兼容）— 方向键即触发 onChange；
   * 'manual'                          — 方向键仅移动焦点，Enter/Space 才触发 onChange。
   */
  activation?: 'auto' | 'manual';
}

export function Tabs(props: TabsProps) {
  const [focusedIndex, setFocusedIndex] = createSignal(-1);
  let containerRef!: HTMLDivElement;

  // 测量激活 tab 的位置，驱动底部 indicator 滑动
  const indicator = useIndicatorTrack(
    () => containerRef,
    () => props.active,
    '[data-active="true"]',
  );

  const focusTabAt = (i: number) => {
    const list = containerRef.querySelectorAll<HTMLElement>('[role="tab"]');
    list[i]?.focus();
  };

  function handleKeyDown(e: KeyboardEvent) {
    const tabs = props.tabs;
    if (tabs.length === 0) return;
    const mode = props.activation ?? 'auto';
    const currentIndex = focusedIndex() >= 0 ? focusedIndex() : tabs.findIndex((t) => t.id === props.active);

    const move = (next: number) => {
      setFocusedIndex(next);
      if (mode === 'auto') props.onChange(tabs[next].id);
      focusTabAt(next);
    };

    if (e.key === 'ArrowRight') {
      e.preventDefault();
      move((currentIndex + 1) % tabs.length);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      move((currentIndex - 1 + tabs.length) % tabs.length);
    } else if (e.key === 'Home') {
      e.preventDefault();
      move(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      move(tabs.length - 1);
    } else if (mode === 'manual' && (e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault();
      const idx = focusedIndex();
      if (idx >= 0 && idx < tabs.length) props.onChange(tabs[idx].id);
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
              id={`tab-${tab.id}`}
              role="tab"
              type="button"
              aria-selected={isActive()}
              aria-controls={`tabpanel-${tab.id}`}
              tabIndex={isActive() ? 0 : -1}
              data-active={isActive() ? 'true' : undefined}
              onClick={() => {
                // 点击 active 化某 tab，应重置已记录的方向键焦点
                setFocusedIndex(-1);
                props.onChange(tab.id);
              }}
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
      {/* Indicator — 通过 transform 滑动；首测 width=0 时隐藏避免入场动画 */}
      <div
        aria-hidden="true"
        class="absolute bottom-[-1px] h-0.5 bg-accent rounded-full pointer-events-none transition-[transform,width] duration-base ease-out-expo"
        style={{
          transform: `translateX(${indicator().left}px)`,
          width: `${indicator().width}px`,
          visibility: indicator().width === 0 ? 'hidden' : 'visible',
        }}
      />
    </div>
  );
}
