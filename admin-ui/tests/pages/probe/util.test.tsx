import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from '@solidjs/testing-library';
import { compact, fmtEps, ratePct, ago, hms, GROUP_META, LockIcon, KebabIcon } from '@/pages/probe/util';

describe('compact', () => {
  it('非有限数一律回 0', () => {
    expect(compact(Number.NaN)).toBe('0');
    expect(compact(Number.POSITIVE_INFINITY)).toBe('0');
    expect(compact(Number.NEGATIVE_INFINITY)).toBe('0');
  });

  it('百万级用 M，千级用 k，按绝对值判断故负数同样缩写', () => {
    expect(compact(1_000_000)).toBe('1.0M');
    expect(compact(2_345_678)).toBe('2.3M');
    expect(compact(-1_500_000)).toBe('-1.5M');
    expect(compact(1_000)).toBe('1.0k');
    expect(compact(1_234)).toBe('1.2k');
    expect(compact(-2_000)).toBe('-2.0k');
  });

  it('千以下四舍五入为整数', () => {
    expect(compact(999)).toBe('999');
    expect(compact(12.4)).toBe('12');
    expect(compact(12.5)).toBe('13');
    expect(compact(0)).toBe('0');
  });
});

describe('fmtEps', () => {
  it('非有限数与非正数回 0', () => {
    expect(fmtEps(Number.NaN)).toBe('0');
    expect(fmtEps(0)).toBe('0');
    expect(fmtEps(-3)).toBe('0');
  });

  it('小于 1 保留两位，1~10 保留一位，10 以上取整', () => {
    expect(fmtEps(0.126)).toBe('0.13');
    expect(fmtEps(1)).toBe('1.0');
    expect(fmtEps(5.67)).toBe('5.7');
    expect(fmtEps(9.99)).toBe('10.0');
    expect(fmtEps(10)).toBe('10');
    expect(fmtEps(12.6)).toBe('13');
  });
});

describe('ratePct', () => {
  it('0~1 小数换算为整数百分比', () => {
    expect(ratePct(0)).toBe(0);
    expect(ratePct(0.256)).toBe(26);
    expect(ratePct(1)).toBe(100);
  });
});

describe('ago', () => {
  const NOW = Date.parse('2026-01-01T12:00:00Z');
  const at = (secsAgo: number) => new Date(NOW - secsAgo * 1000).toISOString();

  afterEach(() => vi.useRealTimers());

  function freeze() {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
  }

  it('null 返回无数据', () => {
    expect(ago(null)).toBe('无数据');
  });

  it('无法解析的时间戳原样回显', () => {
    expect(ago('not-a-timestamp')).toBe('not-a-timestamp');
  });

  it('各时间桶边界', () => {
    freeze();
    expect(ago(at(0))).toBe('just now');
    expect(ago(at(4))).toBe('just now');
    expect(ago(at(5))).toBe('5 秒前');
    expect(ago(at(59))).toBe('59 秒前');
    expect(ago(at(60))).toBe('1 分钟前');
    expect(ago(at(3599))).toBe('59 分钟前');
    expect(ago(at(3600))).toBe('1 小时前');
    expect(ago(at(86399))).toBe('23 小时前');
    expect(ago(at(86400))).toBe('1 天前');
    expect(ago(at(86400 * 9))).toBe('9 天前');
  });

  it('未来时间戳被钳到 0（不出现负数）', () => {
    freeze();
    expect(ago(at(-3600))).toBe('just now');
  });

  it('SQLite 无时区格式也能解析', () => {
    freeze();
    // V8 把 `YYYY-MM-DD HH:MM:SS` 直接按本地时区解析，故用本地时刻构造期望值
    const local = new Date(NOW - 120_000);
    const p = (x: number) => String(x).padStart(2, '0');
    const sqlite = `${local.getFullYear()}-${p(local.getMonth() + 1)}-${p(local.getDate())} ${p(local.getHours())}:${p(local.getMinutes())}:${p(local.getSeconds())}`;
    expect(ago(sqlite)).toBe('2 分钟前');
  });
});

describe('hms', () => {
  it('RFC3339 转本地 HH:MM:SS', () => {
    const ts = '2026-01-01T12:34:56Z';
    const d = new Date(Date.parse(ts));
    const p = (x: number) => String(x).padStart(2, '0');
    expect(hms(ts)).toBe(`${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`);
  });

  it('无时区的本地时刻串按本地时区解析', () => {
    expect(hms('2026-01-01T09:08:07')).toBe('09:08:07');
    expect(hms('2026-01-01 09:08:07')).toBe('09:08:07');
  });

  it('无法解析时截取第 11~19 位', () => {
    expect(hms('not-a-date 12:00:00')).toBe('12:00:00');
  });

  it('无法解析且长度不足时原样回显', () => {
    expect(hms('garbage')).toBe('garbage');
  });
});

describe('GROUP_META / 图标', () => {
  it('三个组各有标题、副标题、样式类与 svg 图标', () => {
    expect(Object.keys(GROUP_META)).toEqual(['behavior', 'learn', 'perf']);
    for (const key of Object.keys(GROUP_META)) {
      const meta = GROUP_META[key];
      expect(meta.title.length).toBeGreaterThan(0);
      expect(meta.sub.length).toBeGreaterThan(0);
      expect(meta.cls).toBe(`is-${key}`);
      const { container, unmount } = render(() => meta.icon);
      expect(container.querySelector('svg')).not.toBeNull();
      unmount();
    }
  });

  it('LockIcon 渲染锁形 svg（矩形 + 锁梁）', () => {
    const { container } = render(() => <LockIcon />);
    const svg = container.querySelector('svg');
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute('stroke')).toBe('currentColor');
    expect(container.querySelector('rect')).not.toBeNull();
    expect(container.querySelector('path')).not.toBeNull();
  });

  it('KebabIcon 渲染三个圆点', () => {
    const { container } = render(() => <KebabIcon />);
    expect(container.querySelectorAll('circle')).toHaveLength(3);
    expect(container.querySelector('svg')!.getAttribute('width')).toBe('14');
  });
});
