import { Show } from 'solid-js';
import { cn } from '@/utils/cn';
import { fatigueStore } from '@/stores/fatigue';

interface FatigueToggleProps {
  /** 首次启用时触发，用于弹出摄像头权限引导 */
  onFirstEnable?: () => void;
}

/**
 * 疲劳检测开关按钮
 * 显示为眼睛图标，点击切换检测状态
 * 检测中时显示脉冲动画
 */
export function FatigueToggle(props: FatigueToggleProps) {
  /**
   * 点击处理逻辑：
   * - 首次启用（enabled = false）且提供了 onFirstEnable 回调时：
   *   调用 onFirstEnable 让父组件弹出摄像头权限引导弹窗，
   *   由父组件决定何时调用 fatigueStore.enable()，此处直接 return 不切换状态。
   * - 其他情况：直接 toggle 检测状态。
   */
  function handleClick() {
    if (!fatigueStore.enabled()) {
      if (props.onFirstEnable) {
        props.onFirstEnable();
        return;
      }
    }
    fatigueStore.toggle();
  }

  return (
    <button
      onClick={handleClick}
      title={fatigueStore.enabled() ? '关闭疲劳检测' : '开启疲劳检测'}
      aria-label={fatigueStore.enabled() ? '关闭疲劳检测' : '开启疲劳检测'}
      class={cn(
        'relative p-2 rounded-lg transition-colors duration-200 cursor-pointer',
        'hover:bg-surface-secondary',
        fatigueStore.enabled() ? 'text-accent' : 'text-content-tertiary',
      )}
    >
      {/* 检测中时的脉冲动画光环 */}
      <Show when={fatigueStore.detecting()}>
        <span class="absolute inset-0 rounded-lg bg-accent/20 animate-pulse" />
      </Show>

      {/* 眼睛 SVG 图标 */}
      <svg
        class="relative w-5 h-5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <Show
          when={fatigueStore.enabled()}
          fallback={
            <>
              {/* 闭合的眼睛（关闭状态） */}
              <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" opacity="0.4" />
              <circle cx="12" cy="12" r="3" opacity="0.4" />
              {/* 斜线表示关闭 */}
              <line x1="4" y1="4" x2="20" y2="20" />
            </>
          }
        >
          {/* 睁开的眼睛（开启状态） */}
          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
          <circle cx="12" cy="12" r="3" />
        </Show>
      </svg>
    </button>
  );
}
