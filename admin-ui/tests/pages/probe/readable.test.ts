import { describe, it, expect } from 'vitest';
import {
  HEALTH_LABEL,
  PROBE_PLAIN,
  GROUP_PLAIN,
  activeProbesHealth,
  eventsHealth,
  queueHealth,
  errorRateHealth,
  overallHealth,
  eventTypeLabel,
  metricLabel,
  metricUnit,
} from '@/pages/probe/readable';
import type { ProbeOverview } from '@/types/probeTelemetry';

function makeOverview(p: {
  active?: number;
  total?: number;
  events?: number;
  deltaPct?: number;
  queue?: number;
  errRate?: number;
} = {}): ProbeOverview {
  return {
    generatedAt: '2026-01-01T00:00:00Z',
    activeProbes: { value: p.active ?? 4, ...(p.total === undefined ? {} : { total: p.total }) },
    events24h: { value: p.events ?? 1234, ...(p.deltaPct === undefined ? {} : { deltaPct: p.deltaPct }) },
    queueBacklog: { value: p.queue ?? 0 },
    collectErrorRate: { value: p.errRate ?? 0.001 },
  };
}

describe('activeProbesHealth', () => {
  it('total=4 时 3 个及以上算正常，2 个降为注意', () => {
    expect(activeProbesHealth(3, 4).level).toBe('normal');
    expect(activeProbesHealth(2, 4).level).toBe('attention');
  });

  it('total<=0 回退按 4 个数据源计算', () => {
    expect(activeProbesHealth(3, 0).level).toBe('normal');
    expect(activeProbesHealth(2, 0).level).toBe('attention');
    expect(activeProbesHealth(2, 0).meaning).toBe('4 个数据源里 2 个在收数据');
    expect(activeProbesHealth(0, -1).meaning).toBe('4 个数据源里 0 个在收数据');
  });

  it('阈值向上取整：total=5 时需要 4 个才正常', () => {
    expect(activeProbesHealth(4, 5).level).toBe('normal');
    expect(activeProbesHealth(3, 5).level).toBe('attention');
  });

  it('全无数据也绝不标异常', () => {
    const h = activeProbesHealth(0, 4);
    expect(h.level).toBe('attention');
    expect(h.hint).toBe('没数据≠故障，可能只是该端暂无这类上报');
  });
});

describe('eventsHealth', () => {
  it('value=0 判为断流嫌疑', () => {
    expect(eventsHealth(0)).toEqual({ level: 'attention', meaning: '近期没有新数据，需确认是否断流' });
  });

  it('value=0 优先于 deltaPct（即使涨幅为正）', () => {
    expect(eventsHealth(0, 3).level).toBe('attention');
  });

  it('跌幅恰好 -50% 触发注意，-49.9% 仍正常', () => {
    expect(eventsHealth(100, -0.5).meaning).toBe('上报量明显下降，建议关注');
    expect(eventsHealth(100, -0.499).level).toBe('normal');
  });

  it('deltaPct 缺省 / null 时按正常处理', () => {
    expect(eventsHealth(100).level).toBe('normal');
    expect(eventsHealth(100, null).level).toBe('normal');
    expect(eventsHealth(100, 0.8).meaning).toBe('数据正常上报中');
  });
});

describe('queueHealth', () => {
  it('0 条与 1~3 条同为正常但文案不同', () => {
    expect(queueHealth(0)).toEqual({ level: 'normal', meaning: '没有任务积压，处理及时', hint: '0 最理想' });
    expect(queueHealth(3).meaning).toBe('少量任务排队，正常');
    expect(queueHealth(3).level).toBe('normal');
  });

  it('4~20 条为注意', () => {
    expect(queueHealth(4).level).toBe('attention');
    expect(queueHealth(4).meaning).toBe('有 4 条任务排队，建议观察趋势');
    expect(queueHealth(20).level).toBe('attention');
  });

  it('超过 20 条为异常', () => {
    const h = queueHealth(21);
    expect(h.level).toBe('abnormal');
    expect(h.meaning).toBe('积压 21 条，可能处理跟不上');
    expect(h.hint).toBe('需排查消费端 / 扩容');
  });
});

describe('errorRateHealth', () => {
  it('低于 0.5% 正常，恰好 0.5% 起为注意', () => {
    expect(errorRateHealth(0.00499).level).toBe('normal');
    expect(errorRateHealth(0.005).level).toBe('attention');
  });

  it('2% 仍是注意，超过 2% 为异常', () => {
    expect(errorRateHealth(0.02).level).toBe('attention');
    expect(errorRateHealth(0.0201).level).toBe('abnormal');
  });

  it('meaning 按每 100 条口径保留两位小数', () => {
    expect(errorRateHealth(0.01234).meaning).toBe('每 100 条上报约 1.23 条出错');
    expect(errorRateHealth(0).level).toBe('normal');
    expect(errorRateHealth(0).meaning).toBe('每 100 条上报约 0.00 条出错');
  });
});

describe('overallHealth', () => {
  it('全正常且队列为 0：无任务积压措辞', () => {
    const r = overallHealth(makeOverview({ active: 4, total: 4, events: 1234, queue: 0, errRate: 0.001 }), '24 小时');
    expect(r.level).toBe('normal');
    expect(r.sentence).toBe(
      '4/4 个数据源在收数据，近 24 小时 上报 1.2k 条，无任务积压，采集错误率 0.10%，整体运转良好。',
    );
  });

  it('全正常但队列 1~3 条：改用「正常范围」措辞，避免与队列卡自相矛盾', () => {
    const r = overallHealth(makeOverview({ queue: 2, events: 500 }), '1 小时');
    expect(r.level).toBe('normal');
    expect(r.sentence).toContain('任务积压 2 条（正常范围）');
    expect(r.sentence).toContain('近 1 小时 上报 500 条');
  });

  it('activeProbes.total 缺省时按 4 计入句子', () => {
    const r = overallHealth(makeOverview({ active: 4, total: undefined }), '24 小时');
    expect(r.sentence).toContain('4/4 个数据源在收数据');
  });

  it('数据源不足 → 注意，句尾为建议关注趋势', () => {
    const r = overallHealth(makeOverview({ active: 1, total: 4 }), '24 小时');
    expect(r).toEqual({ level: 'attention', sentence: '部分数据源无数据，建议关注趋势。' });
  });

  it('上报量为 0 → 近期无上报', () => {
    const r = overallHealth(makeOverview({ events: 0 }), '24 小时');
    expect(r.sentence).toBe('近期无上报，建议关注趋势。');
  });

  it('上报量大跌（非 0）→ 上报量明显下降', () => {
    const r = overallHealth(makeOverview({ events: 500, deltaPct: -0.7 }), '24 小时');
    expect(r.sentence).toBe('上报量明显下降，建议关注趋势。');
  });

  it('队列积压超阈值 → 异常，句尾为请尽快排查', () => {
    const r = overallHealth(makeOverview({ queue: 50 }), '24 小时');
    expect(r).toEqual({ level: 'abnormal', sentence: '任务积压 50 条，请尽快排查。' });
  });

  it('错误率超阈值 → 异常并带百分比', () => {
    const r = overallHealth(makeOverview({ errRate: 0.0345 }), '24 小时');
    expect(r).toEqual({ level: 'abnormal', sentence: '采集错误率 3.45%，请尽快排查。' });
  });

  it('多问题并存时按 数据源→上报→队列→错误率 顺序拼接，异常优先级压过注意', () => {
    const r = overallHealth(
      makeOverview({ active: 0, total: 4, events: 0, queue: 42, errRate: 0.05 }),
      '24 小时',
    );
    expect(r.level).toBe('abnormal');
    expect(r.sentence).toBe('部分数据源无数据、近期无上报、任务积压 42 条、采集错误率 5.00%，请尽快排查。');
  });
});

describe('文案映射', () => {
  it('eventTypeLabel 已知类型译中文，未知回退原值', () => {
    expect(eventTypeLabel('periodic')).toBe('周期上报');
    expect(eventTypeLabel('error_js')).toBe('前端错误');
    expect(eventTypeLabel('unknown_kind')).toBe('unknown_kind');
  });

  it('metricLabel / metricUnit 未知 key 回退原 key / 空串', () => {
    expect(metricLabel('avgResponseTimeMs')).toBe('平均响应');
    expect(metricLabel('nope')).toBe('nope');
    expect(metricUnit('avgResponseTimeMs')).toBe(' ms');
    expect(metricUnit('scrollDepthPct')).toBe('%');
    expect(metricUnit('sessionDurationSecs')).toBe(' 秒');
    expect(metricUnit('clickCount')).toBe('');
  });

  it('HEALTH_LABEL 覆盖三档等级', () => {
    expect(HEALTH_LABEL).toEqual({ normal: '正常', attention: '注意', abnormal: '异常' });
  });

  it('PROBE_PLAIN / GROUP_PLAIN 覆盖看板用到的全部 key', () => {
    expect(Object.keys(PROBE_PLAIN)).toEqual(['click', 'lesson_start', 'word_answer', 'error_js']);
    expect(PROBE_PLAIN.word_answer.meaning).toContain('一条不丢');
    expect(Object.keys(GROUP_PLAIN)).toEqual(['behavior', 'learn', 'perf']);
    expect(GROUP_PLAIN.learn.sub).toBe('学习核心记录');
  });
});
