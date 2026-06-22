import { themeStore } from '@/stores/theme';

export type Tone =
  | 'accent'
  | 'success'
  | 'warning'
  | 'error'
  | 'info'
  | 'muted'
  | 'pink'
  | 'violet';

/* mono ink-on-paper：accent=墨、四状态色(q/w/e/r)=唯一彩色、其余收敛为灰。
   不再有 indigo/pink/violet 彩虹色。 */
const LIGHT_FALLBACKS: Record<Tone, string> = {
  accent: '#0a0a0b',
  success: '#2f8a5b',
  warning: '#b7791f',
  error: '#c0392b',
  info: '#2d6cdf',
  muted: '#9ca3af',
  pink: '#6b7280',
  violet: '#3f4654',
};

const DARK_FALLBACKS: Record<Tone, string> = {
  accent: '#ffffff',
  success: '#5fc98e',
  warning: '#e2b062',
  error: '#f0938c',
  info: '#6fa0ee',
  muted: '#67676e',
  pink: '#a1a1aa',
  violet: '#c5cad3',
};

const CSS_VAR_BY_TONE: Partial<Record<Tone, string>> = {
  accent: '--accent',
  success: '--success',
  warning: '--warning',
  error: '--error',
  info: '--info',
  muted: '--content-tertiary',
};

function cssVar(name: string, fallback: string) {
  if (typeof window === 'undefined') return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

export function chartColor(tone: Tone) {
  const dark = themeStore.effective() === 'dark';
  const fallback = dark ? DARK_FALLBACKS[tone] : LIGHT_FALLBACKS[tone];
  const variable = CSS_VAR_BY_TONE[tone];
  return variable ? cssVar(variable, fallback) : fallback;
}

/* mono 多系列分类色序：墨 → 四状态色 → 灰。克制、可辨、不喧宾夺主。 */
export function chartPalette() {
  return [
    chartColor('accent'),  // 墨
    chartColor('info'),     // r 蓝
    chartColor('success'),  // e 绿
    chartColor('warning'),  // w 琥珀
    chartColor('error'),    // q 红
    chartColor('muted'),    // 灰
  ];
}

/** mono 算法/分类色序（8 档）。经 chartColor 解析为真实色值且随主题翻转，
 *  可安全用于 SVG presentation 属性（var() 在 SVG 属性里不解析）。 */
export function algoPalette(): string[] {
  return [
    chartColor('accent'),  // 墨
    chartColor('info'),    // r 蓝
    chartColor('success'), // e 绿
    chartColor('warning'), // w 琥珀
    chartColor('error'),   // q 红
    chartColor('muted'),   // 灰
    chartColor('pink'),    // 灰（中）
    chartColor('violet'),  // 灰（深/浅）
  ];
}

export function algorithmColor(name: string) {
  const index: Record<string, number> = {
    Heuristic: 0,
    IGE: 1,
    SWD: 2,
    Ensemble: 3,
    MDM: 4,
    Mastery: 5,
  };
  const i = index[name];
  return i === undefined ? chartColor('muted') : chartPalette()[i];
}
