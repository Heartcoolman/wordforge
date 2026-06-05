import { type ParentProps } from 'solid-js';
import { cn } from '@/utils/cn';

interface KbdProps extends ParentProps {
  class?: string;
  size?: 'sm' | 'md';
}

/**
 * 键盘按键提示。用于命令面板、tooltip、shortcut hint。
 * 视觉：mono 字体 + 2px 底 border 模拟物理按键。
 *
 * @example <Kbd>⌘K</Kbd> · <Kbd>ESC</Kbd> · <Kbd>↵</Kbd>
 */
export function Kbd(props: KbdProps) {
  const isSmall = () => props.size === 'sm';
  return (
    <kbd
      class={cn(
        'inline-flex items-center justify-center font-mono font-medium tabular-nums',
        'bg-surface-elevated text-content-secondary border border-border',
        'border-b-2 rounded',
        isSmall() ? 'min-w-4 h-4 px-1 text-[10px]' : 'min-w-[18px] h-[18px] px-1.5 text-[11px]',
        props.class,
      )}
    >
      {props.children}
    </kbd>
  );
}
