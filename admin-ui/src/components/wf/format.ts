/* Formatting helpers — ported from the design handoff WF.* utilities */

export const fmtNum = (n: number | null | undefined): string =>
  n == null ? '—' : n.toLocaleString('en-US');

export const fmtBytes = (b: number | null | undefined): string => {
  if (b == null) return '—';
  const u = ['B', 'KB', 'MB', 'GB'];
  let i = 0;
  let v = b;
  while (v >= 1024 && i < 3) { v /= 1024; i++; }
  return v.toFixed(i ? 1 : 0) + ' ' + u[i];
};

export const fmtDur = (s: number | null | undefined): string => {
  if (s == null) return '—';
  if (s < 60) return Math.round(s) + 's';
  if (s < 3600) return Math.round(s / 60) + 'm';
  if (s < 86400) return (s / 3600).toFixed(1) + 'h';
  return Math.round(s / 86400) + 'd';
};

export const fmtTime = (iso: string | number | null | undefined): string => {
  if (iso == null) return '—';
  try { return new Date(iso).toLocaleString('zh-CN', { hour12: false }); } catch { return String(iso); }
};

export const fmtAgo = (iso: string | number | null | undefined): string => {
  if (iso == null) return '—';
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (Number.isNaN(s)) return '—';
  if (s < 60) return Math.round(s) + ' 秒前';
  if (s < 3600) return Math.round(s / 60) + ' 分钟前';
  if (s < 86400) return Math.round(s / 3600) + ' 小时前';
  return Math.round(s / 86400) + ' 天前';
};

export const fmtPct = (v: number | null | undefined, digits = 1): string =>
  v == null ? '—' : (v * 100).toFixed(digits) + '%';
