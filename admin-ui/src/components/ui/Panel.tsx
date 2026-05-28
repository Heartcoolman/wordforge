import { type JSX, Show } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import { Card } from './Card';

interface PanelProps {
  title: string;
  actions?: JSX.Element;
  children: JSX.Element;
  aside?: JSX.Element;
  /** 标题语义层级（1-4），默认 2。仅影响 a11y/SEO，视觉样式保持 text-headline */
  headingLevel?: 1 | 2 | 3 | 4;
}

export function Panel(props: PanelProps) {
  const heading = () => `h${props.headingLevel ?? 2}` as 'h1' | 'h2' | 'h3' | 'h4';

  return (
    <Card variant="elevated" class="flex flex-col">
      <div class="flex items-center justify-between pb-4 mb-4 border-b border-border-hairline">
        <Dynamic component={heading()} class="text-headline text-content">{props.title}</Dynamic>
        <Show when={props.actions}>{props.actions}</Show>
      </div>
      {/* aside 缺席时主区独占全宽，避免 lg:grid-cols-4 + 空 aside 造成 1/4 列留白 */}
      <Show
        when={props.aside}
        fallback={<div class="min-w-0">{props.children}</div>}
      >
        <div class="grid grid-cols-1 lg:grid-cols-4 gap-4">
          <div class="lg:col-span-3 min-w-0">{props.children}</div>
          <div class="lg:col-span-1 flex flex-col gap-3">{props.aside}</div>
        </div>
      </Show>
    </Card>
  );
}
