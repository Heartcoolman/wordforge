import { createSignal, Show, type JSX } from 'solid-js';

/**
 * 通用折叠区。标题行可显示 badge（如版本号 / 数量），点击折叠/展开内容。
 *
 * - ARIA：标题行是 `<button>`，`aria-expanded` 反映当前状态，键盘 space/enter 触发
 * - 默认折叠；`defaultOpen` 可设初始展开
 * - 受控信号在内部，父组件只通过 `onToggle` 观察
 */
interface Props {
  title: string;
  badge?: string | null;
  defaultOpen?: boolean;
  onToggle?: (open: boolean) => void;
  children: JSX.Element;
}

export function Collapsible(props: Props) {
  const [open, setOpen] = createSignal(props.defaultOpen ?? false);

  const toggle = () => {
    const next = !open();
    setOpen(next);
    props.onToggle?.(next);
  };

  return (
    <div class="border border-border-hairline rounded-lg overflow-hidden">
      <button
        type="button"
        class="w-full flex items-center justify-between px-4 py-3 hover:bg-surface-secondary/60 transition-colors duration-fast"
        aria-expanded={open()}
        onClick={toggle}
      >
        <span class="flex items-center gap-2 font-medium text-content">
          <span
            aria-hidden
            class="inline-block transition-transform duration-fast ease-out-expo"
            style={{ transform: open() ? 'rotate(90deg)' : 'rotate(0deg)' }}
          >
            ▸
          </span>
          {props.title}
        </span>
        <Show when={props.badge}>
          <span class="text-xs px-2 py-0.5 rounded-full bg-accent text-white font-mono">
            {props.badge}
          </span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="p-4 border-t border-border-hairline">{props.children}</div>
      </Show>
    </div>
  );
}
