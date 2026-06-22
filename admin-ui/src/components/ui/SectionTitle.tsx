import { type ParentProps, Show } from 'solid-js';
import { cn } from '@/utils/cn';

interface SectionTitleProps extends ParentProps {
  /** 二级标题，必填 */
  title: string;
  /** 副说明文本，右侧灰色小字 */
  sub?: string;
  /** 标题级别：h2（页面分区）或 h3（卡片内），默认 h2 */
  as?: 'h2' | 'h3';
  /** 右侧自定义内容（如 action 按钮、版本 chip），优先级高于 sub */
  actions?: any;
  class?: string;
}

/**
 * 页面分区标题。统一二级 / 三级标题视觉，避免散落不一致的 font-weight / margin。
 *
 * @example
 *   <SectionTitle title="设计系统总览" sub="mono · Sora + IBM Plex Mono" />
 *   <SectionTitle title="Worker 健康" actions={<Button size="sm">刷新</Button>} />
 */
export function SectionTitle(props: SectionTitleProps) {
  const h3Class = 'text-headline';
  const h2Class = 'text-base font-semibold tracking-[-0.012em]';

  return (
    <div class={cn('flex items-baseline gap-3 my-4 first:mt-0', props.class)}>
      <Show when={props.as === 'h3'} fallback={<h2 class={h2Class}>{props.title}</h2>}>
        <h3 class={h3Class}>{props.title}</h3>
      </Show>
      <Show when={props.actions} fallback={
        <Show when={props.sub}>
          <span class="text-content-tertiary text-[12.5px]">{props.sub}</span>
        </Show>
      }>
        <div class="ml-auto flex items-center gap-2">{props.actions}</div>
      </Show>
      {props.children}
    </div>
  );
}
