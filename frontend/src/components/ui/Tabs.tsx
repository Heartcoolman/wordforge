import { For, type JSX } from 'solid-js';
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
  return (
    <div class={cn('flex border-b border-border', props.class)}>
      <For each={props.tabs}>
        {(tab) => (
          <button
            onClick={() => props.onChange(tab.id)}
            class={cn(
              'flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium transition-colors cursor-pointer',
              'border-b-2 -mb-px',
              props.active === tab.id
                ? 'border-accent text-accent'
                : 'border-transparent text-content-secondary hover:text-content hover:border-border',
            )}
          >
            {tab.icon}
            {tab.label}
          </button>
        )}
      </For>
    </div>
  );
}
