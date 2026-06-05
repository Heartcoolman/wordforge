import type { JSX } from 'solid-js';

interface TechTipProps {
  /** 提示内容（技术口径 / 业务解释，可为富文本）。 */
  children: JSX.Element;
  /** 无障碍标签，默认"说明"。 */
  label?: string;
  /** 气泡宽度，默认 260px。 */
  width?: string;
  /** 气泡水平对齐：居中（默认）或右对齐（贴近视口右缘时用）。 */
  align?: 'center' | 'right';
  /** 气泡方位：默认上方；靠近页顶的元素用 'bottom'。 */
  placement?: 'top' | 'bottom';
}

/** 轻量"说明"悬浮提示：主文案旁放一个 ⓘ，鼠标悬停或键盘聚焦时显示。
 *  纯 CSS 显隐（:hover / :focus-within），不依赖第三方库；tabindex 使其键盘可达。
 *  让非技术客服把光标移上去就能看懂某个数字 / 术语的含义。 */
export function TechTip(props: TechTipProps) {
  const cls = () =>
    `pb-tip${props.align === 'right' ? ' is-right' : ''}${props.placement === 'bottom' ? ' is-below' : ''}`;
  return (
    <span class={cls()} tabindex="0" role="button" aria-label={props.label ?? '说明'}>
      <svg class="pb-tip__ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="11" x2="12" y2="16" />
        <line x1="12" y1="8" x2="12.01" y2="8" />
      </svg>
      <span class="pb-tip__pop" style={{ '--tip-w': props.width ?? '260px' }} role="tooltip">
        {props.children}
      </span>
    </span>
  );
}
