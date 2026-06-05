/**
 * 数据探针页"可读化"单一事实源：健康判读阈值、整体健康聚合、事件 / 指标 / 探针的中文文案。
 *
 * 设计要点（语义正确性优先，避免误导非技术客服）：
 *  - 活跃探针：0 数据≠故障，多数≠健康，仅作数据源覆盖度，绝不轻易标红。
 *  - 上报量：涨跌都依赖时段，真正坏信号是"长期 0"；故只标 正常 / 注意，不标异常。
 *  - 队列积压、采集错误率：含义明确（越低越好），允许标红。
 *  - 阈值均为经验法则，非工程规格。
 */

import { compact } from './util';
import type { ProbeOverview } from '@/types/probeTelemetry';

export type HealthLevel = 'normal' | 'attention' | 'abnormal';

export const HEALTH_LABEL: Record<HealthLevel, string> = {
  normal: '正常',
  attention: '注意',
  abnormal: '异常',
};

export interface KpiHealth {
  level: HealthLevel;
  /** 卡面主解读（一句话）。 */
  meaning: string;
  /** 卡面次级提示（正常区间 / 注意要点），可空。 */
  hint?: string;
}

/** 活跃探针：>=75% 正常；>=1 注意；0 异常（数据源全无）。 */
export function activeProbesHealth(value: number, total: number): KpiHealth {
  const t = total > 0 ? total : 4;
  // 0 数据≠故障（可能低峰/夜间/新窗口），绝不标红：只在 正常/注意 间取值。
  const level: HealthLevel = value >= Math.ceil(t * 0.75) ? 'normal' : 'attention';
  return {
    level,
    meaning: `${t} 个数据源里 ${value} 个在收数据`,
    hint: '没数据≠故障，可能只是该端暂无这类上报',
  };
}

/** 上报量：0 → 注意（疑似断流）；大跌(≤-50%) → 注意；其余正常。不标异常。 */
export function eventsHealth(value: number, deltaPct?: number | null): KpiHealth {
  if (value === 0) {
    return { level: 'attention', meaning: '近期没有新数据，需确认是否断流' };
  }
  if (deltaPct != null && deltaPct <= -0.5) {
    return { level: 'attention', meaning: '上报量明显下降，建议关注' };
  }
  return { level: 'normal', meaning: '数据正常上报中' };
}

/** 队列积压：≤3 正常；≤20 注意；>20 异常。 */
export function queueHealth(value: number): KpiHealth {
  if (value <= 3) {
    return { level: 'normal', meaning: value === 0 ? '没有任务积压，处理及时' : '少量任务排队，正常', hint: '0 最理想' };
  }
  if (value <= 20) {
    return { level: 'attention', meaning: `有 ${value} 条任务排队，建议观察趋势`, hint: '持续增长才需警惕' };
  }
  return { level: 'abnormal', meaning: `积压 ${value} 条，可能处理跟不上`, hint: '需排查消费端 / 扩容' };
}

/** 采集错误率：<0.5% 正常；≤2% 注意；>2% 异常。入参为 0~1 小数。 */
export function errorRateHealth(rate: number): KpiHealth {
  const pct = rate * 100;
  const level: HealthLevel = pct < 0.5 ? 'normal' : pct <= 2 ? 'attention' : 'abnormal';
  return {
    level,
    meaning: `每 100 条上报约 ${pct.toFixed(2)} 条出错`,
    hint: '低于 0.5% 算正常 · 超过 2% 需排查 · 越低越好',
  };
}

/** 整体健康：任一异常→异常；否则任一注意→注意；否则正常。附一句话总结。 */
export function overallHealth(o: ProbeOverview, winLabel: string): { level: HealthLevel; sentence: string } {
  const a = activeProbesHealth(o.activeProbes.value, o.activeProbes.total ?? 4);
  const e = eventsHealth(o.events24h.value, o.events24h.deltaPct);
  const q = queueHealth(o.queueBacklog.value);
  const r = errorRateHealth(o.collectErrorRate.value);
  const levels = [a.level, e.level, q.level, r.level];
  const level: HealthLevel = levels.includes('abnormal')
    ? 'abnormal'
    : levels.includes('attention')
      ? 'attention'
      : 'normal';

  // 句子不含"系统健康度X"前缀（由 banner 标签承担），仅给细节。
  const errPct = (o.collectErrorRate.value * 100).toFixed(2);
  if (level === 'normal') {
    // 队列文案按真实值，避免 1~3 条积压时与队列卡"少量排队"自相矛盾。
    const queuePhrase = o.queueBacklog.value === 0 ? '无任务积压' : `任务积压 ${o.queueBacklog.value} 条（正常范围）`;
    return {
      level,
      sentence: `${o.activeProbes.value}/${o.activeProbes.total ?? 4} 个数据源在收数据，近 ${winLabel} 上报 ${compact(o.events24h.value)} 条，${queuePhrase}，采集错误率 ${errPct}%，整体运转良好。`,
    };
  }
  const issues: string[] = [];
  if (a.level !== 'normal') issues.push('部分数据源无数据');
  if (e.level !== 'normal') issues.push(o.events24h.value === 0 ? '近期无上报' : '上报量明显下降');
  if (q.level !== 'normal') issues.push(`任务积压 ${o.queueBacklog.value} 条`);
  if (r.level !== 'normal') issues.push(`采集错误率 ${errPct}%`);
  return {
    level,
    sentence: issues.join('、') + (level === 'abnormal' ? '，请尽快排查。' : '，建议关注趋势。'),
  };
}

// ── 实时事件流：事件类型 / 指标的中文标签 ──────────────────────────

const EVENT_TYPE_LABELS: Record<string, string> = {
  periodic: '周期上报',
  session_start: '会话开始',
  on_demand: '按需上报',
  error_js: '前端错误',
  error: '错误',
};

/** event_type → 中文标签；未知类型回退原值。 */
export function eventTypeLabel(type: string): string {
  return EVENT_TYPE_LABELS[type] ?? type;
}

const METRIC_LABELS: Record<string, string> = {
  actionsPerMin: '每分钟操作',
  avgResponseTimeMs: '平均响应',
  sessionDurationSecs: '会话时长',
  clickCount: '点击数',
  scrollDepthPct: '滚动深度',
  routeChanges: '页面切换',
  visibilityChanges: '前后台切换',
  currentRoute: '当前页面',
  errorCount: '错误数',
};

const METRIC_UNITS: Record<string, string> = {
  avgResponseTimeMs: ' ms',
  sessionDurationSecs: ' 秒',
  scrollDepthPct: '%',
};

/** 指标 key → 中文标签；未知 key 回退原 key。 */
export function metricLabel(key: string): string {
  return METRIC_LABELS[key] ?? key;
}

/** 指标 key → 单位后缀（已含前导空格 / 百分号）。 */
export function metricUnit(key: string): string {
  return METRIC_UNITS[key] ?? '';
}

// ── 探针 / 探针组：业务文案（替换 table.column 技术描述）──────────────

export const PROBE_PLAIN: Record<string, { desc: string; meaning: string }> = {
  click: { desc: '用户在界面上的点击、滑动等操作', meaning: '数字越大说明 App 被用得越频繁' },
  lesson_start: { desc: '用户每次开始学习一节课', meaning: '学习的起点，必须全量保留' },
  word_answer: { desc: '用户每次做单词题的作答', meaning: 'AI 个性化出题的训练数据，一条不丢' },
  error_js: { desc: '前端运行时报的错误 / 崩溃', meaning: '越多说明 App 当前越不稳定' },
};

export const GROUP_PLAIN: Record<string, { sub: string; meaning: string }> = {
  behavior: { sub: '用户操作行为', meaning: '点击、滑动等交互，可按比例采样以省资源' },
  learn: { sub: '学习核心记录', meaning: '开始学习、答题等过程，强制全量保留' },
  perf: { sub: '性能与错误', meaning: '前端错误、卡顿等问题，错误不可丢' },
};
