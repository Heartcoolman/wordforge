import type { JSX } from 'solid-js';

/**
 * sx — convert a React-style (camelCase, numeric px) style object into a
 * Solid-compatible style object (kebab-case keys, units appended to numbers).
 * Lets ported handoff markup keep its original inline-style objects verbatim.
 */
const UNITLESS = new Set([
  'opacity', 'z-index', 'font-weight', 'line-height', 'flex', 'flex-grow',
  'flex-shrink', 'order', 'zoom', 'tab-size', 'aspect-ratio', 'grid-row',
  'grid-column', 'column-count', 'fill-opacity', 'stroke-opacity',
  'stroke-width', 'stroke-dashoffset', 'animation-iteration-count',
]);

const kebab = (k: string) => k.replace(/[A-Z]/g, (m) => '-' + m.toLowerCase());

export function sx(
  obj: Record<string, string | number | null | undefined | false> | undefined,
): JSX.CSSProperties {
  const out: Record<string, string> = {};
  if (!obj) return out as JSX.CSSProperties;
  for (const k in obj) {
    const v = obj[k];
    if (v == null || v === false) continue;
    const key = kebab(k);
    out[key] = typeof v === 'number' && !UNITLESS.has(key) ? `${v}px` : String(v);
  }
  return out as JSX.CSSProperties;
}

/** cx — tiny className joiner (matches the handoff `cx`) */
export const cx = (...a: Array<string | false | null | undefined>) => a.filter(Boolean).join(' ');
