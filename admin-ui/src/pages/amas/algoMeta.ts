import { chartColor, type Tone } from '@/lib/chartTheme';

/** 9 决策/记忆算法的展示名。后端 routing_algo 落库为小写，按小写键映射。 */
const ALGO_LABEL: Record<string, string> = {
  ensemble: 'ensemble',
  mdm: 'MDM',
  swd: 'SWD',
  ige: 'IGE',
  ssp: 'SSP',
  heuristic: 'heuristic',
  mtp: 'MTP',
  iad: 'IAD',
  mastery: 'Mastery',
};

/** mono 配色：墨 + 四状态色(q/w/e/r) + 灰。经 chartColor 主题自适应解析为真实色值。 */
const ALGO_TONE: Record<string, Tone> = {
  ensemble: 'accent',
  mdm: 'success',
  swd: 'info',
  ige: 'warning',
  ssp: 'error',
  heuristic: 'muted',
  mtp: 'pink',
  iad: 'violet',
  mastery: 'success',
};

export function algoMeta(name: string): { label: string; color: string } {
  const key = (name || '').toLowerCase();
  const tone = ALGO_TONE[key];
  return {
    label: ALGO_LABEL[key] ?? (name || '—'),
    color: tone ? chartColor(tone) : chartColor('muted'),
  };
}
