import { Show } from 'solid-js';
import { ParamField } from '@/pages/amas/ParamField';
import type { ParamMeta } from '@/pages/amas/schema';
import { cn } from '@/utils/cn';

interface ParamCardProps {
  meta: ParamMeta;
  value: unknown;
  error?: string;
  onChange: (v: unknown) => void;
  /** compact=true 隐藏 description / affects 行(SectionPanel 已用) */
  compact?: boolean;
}

/**
 * 单参数卡片:elevation-1 + radius-lg + padding-4。
 *
 * 视觉状态(对齐 amas-config.html .param-card):
 * - 默认: surface-elevated + transparent ring
 * - 已修改: warning ring + warning-light 微底色
 * - 校验错误: error ring 高亮
 *
 * 复用 ParamField 完整能力(Switch / Input+Slider / default/tuned/AI / explanation)。
 * 只负责"单卡视觉",不重复其内逻辑。
 */
export function ParamCard(props: ParamCardProps) {
  const isModified = () => !Object.is(props.value, props.meta.default);
  const hasError = () => Boolean(props.error);

  return (
    <div
      class={cn(
        'rounded-lg bg-surface-elevated p-4 shadow-elevation-1',
        'ring-1 transition-[box-shadow,background-color,border-color] duration-fast ease-out-expo',
        hasError() && 'ring-error',
        !hasError() && isModified() && 'ring-warning bg-warning-light/40',
        !hasError() && !isModified() && 'ring-transparent',
      )}
    >
      <ParamField
        meta={props.meta}
        value={props.value}
        error={props.error}
        onChange={props.onChange}
        compact={props.compact}
      />
      {/* 已修改时显示 path mono 小字便于对齐设计图 "已修改" badge 的信息密度 */}
      <Show when={isModified() && !props.compact}>
        <div class="mt-2 pt-2 border-t border-border-hairline">
          <code class="text-[10px] font-mono text-content-tertiary">{props.meta.path}</code>
        </div>
      </Show>
    </div>
  );
}
