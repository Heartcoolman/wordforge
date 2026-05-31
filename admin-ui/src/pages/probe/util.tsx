import type { JSX } from 'solid-js';

/** 大数缩写：1234 → 1.2k，1.5M。整数 < 1000 原样。 */
export function compact(n: number): string {
  if (!Number.isFinite(n)) return '0';
  const abs = Math.abs(n);
  if (abs >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(Math.round(n));
}

/** EPS（事件/秒）格式：< 1 保留两位，否则一位。 */
export function fmtEps(eps: number): string {
  if (!Number.isFinite(eps) || eps <= 0) return '0';
  if (eps < 1) return eps.toFixed(2);
  if (eps < 10) return eps.toFixed(1);
  return String(Math.round(eps));
}

/** 采样率 0~1 → 百分比整数字符串（用于 pct 显示）。 */
export function ratePct(rate: number): number {
  return Math.round(rate * 100);
}

/** 相对时间："just now" / "N 秒前" / "N 分钟前" / "N 小时前" / "N 天前"。 */
export function ago(ts: string | null): string {
  if (!ts) return '无数据';
  const t = parseTs(ts);
  if (t == null) return ts;
  const delta = Math.max(0, Math.floor((Date.now() - t) / 1000));
  if (delta < 5) return 'just now';
  if (delta < 60) return `${delta} 秒前`;
  if (delta < 3600) return `${Math.floor(delta / 60)} 分钟前`;
  if (delta < 86400) return `${Math.floor(delta / 3600)} 小时前`;
  return `${Math.floor(delta / 86400)} 天前`;
}

/** HH:MM:SS（本地时区），用于事件流时间列。 */
export function hms(ts: string): string {
  const t = parseTs(ts);
  if (t == null) return ts.slice(11, 19) || ts;
  const d = new Date(t);
  const p = (x: number) => String(x).padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** 解析 RFC3339 或 SQLite `YYYY-MM-DD HH:MM:SS`（UTC）→ epoch ms。 */
function parseTs(ts: string): number | null {
  let v = Date.parse(ts);
  if (!Number.isNaN(v)) return v;
  // SQLite datetime('now')：无时区，按 UTC 处理
  v = Date.parse(ts.replace(' ', 'T') + 'Z');
  return Number.isNaN(v) ? null : v;
}

/** 组元数据：标题 + 副标题 + 图标（与后端 group key 对齐）。 */
export const GROUP_META: Record<string, { title: string; sub: string; cls: string; icon: JSX.Element }> = {
  behavior: {
    title: '用户行为',
    sub: 'telemetry_summaries · 可调采样',
    cls: 'is-behavior',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="3" /><path d="M12 1v6m0 10v6m11-11h-6m-10 0H1" />
      </svg>
    ),
  },
  learn: {
    title: '学习事件',
    sub: 'learning_sessions / learning_records · 100% 强制保留',
    cls: 'is-learn',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" />
        <path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" />
      </svg>
    ),
  },
  perf: {
    title: '性能与错误',
    sub: 'telemetry_summaries.error_count · 错误不可丢',
    cls: 'is-perf',
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <polyline points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
      </svg>
    ),
  },
};

/** 三点 / 锁 图标（行操作位）。 */
export function LockIcon(): JSX.Element {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <rect x="3" y="11" width="18" height="11" rx="2" />
      <path d="M7 11V7a5 5 0 0 1 10 0v4" />
    </svg>
  );
}

/** 横向三点（kebab）图标，行「详情」按钮用。 */
export function KebabIcon(): JSX.Element {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="1" /><circle cx="19" cy="12" r="1" /><circle cx="5" cy="12" r="1" />
    </svg>
  );
}
