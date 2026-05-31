/** 8 决策/记忆算法的展示名与配色，移植自设计稿 amas-metrics.html 的甜甜圈 oklch 调色板。
 * 后端 routing_algo 落库为小写，这里按小写键映射。 */
export const ALGO_META: Record<string, { label: string; color: string }> = {
  ensemble: { label: 'ensemble', color: 'oklch(58% 0.20 269)' },
  mdm: { label: 'MDM', color: 'oklch(66% 0.16 162)' },
  swd: { label: 'SWD', color: 'oklch(64% 0.16 245)' },
  ige: { label: 'IGE', color: 'oklch(72% 0.16 75)' },
  ssp: { label: 'SSP', color: 'oklch(56% 0.22 300)' },
  heuristic: { label: 'heuristic', color: 'oklch(64% 0.18 25)' },
  mtp: { label: 'MTP', color: 'oklch(70% 0.15 195)' },
  iad: { label: 'IAD', color: 'oklch(60% 0.18 330)' },
  mastery: { label: 'Mastery', color: 'oklch(62% 0.17 140)' },
};

export function algoMeta(name: string): { label: string; color: string } {
  return ALGO_META[name.toLowerCase()] ?? { label: name || '—', color: 'var(--content-tertiary)' };
}
