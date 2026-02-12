import { createSignal, For, type JSX } from 'solid-js';
import { cn } from '@/utils/cn';

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
    <div class={cn('flex border-b border-border', props.class)} role="tablist" onKeyDown={handleKeyDown}>
      <For each={props.tabs}>
        {(tab) => {
          const isActive = () => props.active === tab.id;
          return (
            <button
              role="tab"
              aria-selected={isActive()}
              tabIndex={isActive() ? 0 : -1}
              onClick={() => props.onChange(tab.id)}
              class={cn(
                'flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium transition-colors cursor-pointer',
                'border-b-2 -mb-px',
                isActive()
                  ? 'border-accent text-accent'
                  : 'border-transparent text-content-secondary hover:text-content hover:border-border',
              )}
            >
              {tab.icon}
              {tab.label}
            </button>
          );
        }}
      </For>
    </div>
  );
}
