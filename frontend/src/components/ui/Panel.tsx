import { type JSX, Show } from 'solid-js';
import { Card } from './Card';

interface PanelProps {
  title: string;
  actions?: JSX.Element;
  children: JSX.Element;
  aside?: JSX.Element;
}

export function Panel(props: PanelProps) {
  return (
    <Card variant="elevated" class="flex flex-col">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-lg font-semibold text-content">{props.title}</h2>
        <Show when={props.actions}>{props.actions}</Show>
      </div>
      <div class="grid grid-cols-1 lg:grid-cols-4 gap-4">
        <div class="lg:col-span-3 min-w-0">{props.children}</div>
        <Show when={props.aside}>
          <div class="lg:col-span-1 flex flex-col gap-3">{props.aside}</div>
        </Show>
      </div>
    </Card>
  );
}
