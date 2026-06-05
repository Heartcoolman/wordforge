import { type ParentProps, Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface PageHeaderProps extends ParentProps {
  /** H1 主标题(必填)。 */
  title: string;
  /** 副描述,放在标题下方,max-width 65ch。 */
  desc?: string;
  /** 右侧 action 区(按钮组),flex-wrap 自适应。 */
  actions?: any;
  class?: string;
}

/**
 * 轻量 page-header:H1 + desc + 右侧 actions。
 *
 * 与 HeroCard 的区别:
 * - HeroCard 是 elevation-2 大型 hero(eyebrow + display 标题 + halo + meta-grid),
 *   适合 dashboard / analytics / users 等"数据展示型"首屏。
 * - PageHeader 是无背景的轻量页头,适合 amas-config / clients / monitoring 等
 *   "运维操作型"页面,降低视觉权重把空间让给主表/编辑器。
 *
 * 对齐 amas-config.html / clients.html 的 .page-header .row.spread 结构。
 */
export function PageHeader(props: PageHeaderProps) {
  return (
    <header
      class={cn(
        'flex flex-wrap items-start justify-between gap-x-6 gap-y-3',
        'animate-fade-in-up',
        props.class,
      )}
    >
      <div class="flex-1 min-w-0">
        <h1 class="text-[clamp(22px,2.2vw,28px)] font-bold leading-tight tracking-[-0.02em] text-content">
          {props.title}
        </h1>
        <Show when={props.desc}>
          <p class="mt-2 max-w-[65ch] text-sm leading-relaxed text-content-secondary">
            {props.desc}
          </p>
        </Show>
        {props.children}
      </div>
      <Show when={props.actions}>
        <div class="flex flex-wrap items-center gap-2">{props.actions}</div>
      </Show>
    </header>
  );
}
