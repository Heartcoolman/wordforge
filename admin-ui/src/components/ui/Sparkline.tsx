import { Show, createMemo, type JSX } from 'solid-js';
import { cn } from '@/utils/cn';

interface SparklineProps {
  /** 数据序列；为空或长度 < 2 时不渲染 */
  data: number[];
  /** SVG 视口宽度，默认 80 */
  w?: number;
  /** SVG 视口高度，默认 28 */
  h?: number;
  /** 折线颜色，默认 var(--accent) */
  stroke?: string;
  /** 填充色，默认 stroke 的 12% mix */
  fill?: string;
  /** 折线宽度，默认 1.5 */
  strokeWidth?: number;
  class?: string;
  /** SVG aria-label；不指定则 role 设为 presentation */
  ariaLabel?: string;
}

/**
 * 纯 SVG 单条折线 sparkline（无外部依赖）。
 * 仅做"显示一段时序数据走向"，不带轴、tooltip、点位标注。
 * 数据点超过 1 才渲染，否则返回空（占位由调用方处理）。
 */
export function Sparkline(props: SparklineProps): JSX.Element {
  const path = createMemo(() => {
    const data = props.data;
    if (!data || data.length < 2) return null;
    const w = props.w ?? 80;
    const h = props.h ?? 28;
    const max = Math.max(...data);
    const min = Math.min(...data);
    const range = max - min || 1;
    const dx = w / (data.length - 1);
    const pts = data.map((v, i) => {
      const x = i * dx;
      // 顶/底各留 2px 内边距，避免线条贴边
      const y = h - ((v - min) / range) * (h - 4) - 2;
      return [x, y] as const;
    });
    const line = 'M' + pts.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(' L');
    const area = `${line} L${w.toFixed(1)},${h} L0,${h} Z`;
    return { line, area, w, h };
  });

  return (
    <Show when={path()}>
      {(p) => (
        <svg
          class={cn('block', props.class)}
          viewBox={`0 0 ${p().w} ${p().h}`}
          preserveAspectRatio="none"
          role={props.ariaLabel ? 'img' : 'presentation'}
          aria-label={props.ariaLabel}
        >
          <path
            d={p().area}
            fill={props.fill ?? `color-mix(in oklab, ${props.stroke ?? 'var(--accent)'} 12%, transparent)`}
            stroke="none"
          />
          <path
            d={p().line}
            fill="none"
            stroke={props.stroke ?? 'var(--accent)'}
            stroke-width={props.strokeWidth ?? 1.5}
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      )}
    </Show>
  );
}
