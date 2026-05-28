import { Show, type JSX } from 'solid-js';
import { cn } from '@/utils/cn';

interface EmptyProps {
  icon?: JSX.Element;
  title?: string;
  description?: string;
  action?: JSX.Element;
  class?: string;
}

// stagger fade-in-up 通用配置，fill-mode: forwards 让动画结束后清空 transform，
// 避免与子元素（如按钮 hover translate）冲突
const STAGGER_BASE = { 'animation-fill-mode': 'forwards' as const };
const stagger = (delay: string): JSX.CSSProperties => ({ ...STAGGER_BASE, 'animation-delay': delay });

export function Empty(props: EmptyProps) {
  return (
    // 父容器去 animate-fade-in，避免与子项 fade-in-up 双层叠加闪烁；
    // 子项 stagger 自带视觉节奏
    <div class={cn('flex flex-col items-center justify-center py-12 px-4 text-center', props.class)}>
      <Show when={props.icon} fallback={
        <svg
          class="w-12 h-12 text-content-tertiary mb-4 animate-fade-in-up"
          style={STAGGER_BASE}
          fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"
        >
          <path stroke-linecap="round" stroke-linejoin="round" d="M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4" />
        </svg>
      }>
        <div class="mb-4 animate-fade-in-up" style={STAGGER_BASE}>{props.icon}</div>
      </Show>
      <Show when={props.title}>
        <h3 class="text-base font-medium text-content-secondary mb-1 animate-fade-in-up" style={stagger('80ms')}>{props.title}</h3>
      </Show>
      <Show when={props.description}>
        <p class="text-sm text-content-tertiary max-w-sm animate-fade-in-up" style={stagger('160ms')}>{props.description}</p>
      </Show>
      <Show when={props.action}>
        {/* inline-block 限制 wrapper 不撑满；fill-mode: forwards 保证 hover transform 不被 keyframes 残留干扰 */}
        <div class="mt-4 inline-block animate-fade-in-up" style={stagger('240ms')}>{props.action}</div>
      </Show>
    </div>
  );
}
