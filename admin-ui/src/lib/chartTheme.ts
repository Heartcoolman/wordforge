import { themeStore } from '@/stores/theme';

type Tone =
  | 'accent'
  | 'success'
  | 'warning'
  | 'error'
  | 'info'
  | 'muted'
  | 'pink'
  | 'violet';

const LIGHT_FALLBACKS: Record<Tone, string> = {
  accent: '#6366f1',
  success: '#10b981',
  warning: '#f59e0b',
  error: '#ef4444',
  info: '#3b82f6',
  muted: '#94a3b8',
  pink: '#ec4899',
  violet: '#8b5cf6',
};

const DARK_FALLBACKS: Record<Tone, string> = {
  accent: '#818cf8',
  success: '#34d399',
  warning: '#fbbf24',
  error: '#f87171',
  info: '#60a5fa',
  muted: '#64748b',
  pink: '#f472b6',
  violet: '#a78bfa',
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

export function chartPalette() {
  return [
    chartColor('accent'),
    chartColor('success'),
    chartColor('warning'),
    chartColor('pink'),
    chartColor('info'),
    chartColor('violet'),
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
