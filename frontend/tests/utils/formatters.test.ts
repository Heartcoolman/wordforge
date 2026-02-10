import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  formatDate,
  formatDateTime,
  formatRelativeTime,
  formatPercent,
  formatResponseTime,
  formatNumber,
  truncate,
} from '@/utils/formatters';

describe('formatDate', () => {
  it('formats a valid ISO string', () => {
    const result = formatDate('2024-03-15T10:30:00Z');
    expect(result).toMatch(/2024/);
    expect(result).toMatch(/03/);
    expect(result).toMatch(/15/);
  });

  it('returns "-" for invalid string', () => {
    expect(formatDate('not-a-date')).toBe('-');
  });
});

describe('formatDateTime', () => {
  it('formats a valid ISO string with time', () => {
    const result = formatDateTime('2024-03-15T10:30:00Z');
    expect(result).toMatch(/2024/);
    expect(result).toMatch(/03/);
  });

  it('returns "-" for invalid string', () => {
    expect(formatDateTime('invalid')).toBe('-');
  });
});

describe('formatRelativeTime', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2024-06-01T12:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('returns "刚刚" for < 60 seconds ago', () => {
    const iso = new Date(Date.now() - 30 * 1000).toISOString();
    expect(formatRelativeTime(iso)).toBe('刚刚');
  });

  it('returns "分钟前" for minutes ago', () => {
    const iso = new Date(Date.now() - 5 * 60 * 1000).toISOString();
    expect(formatRelativeTime(iso)).toBe('5 分钟前');
  });

  it('returns "小时前" for hours ago', () => {
    const iso = new Date(Date.now() - 3 * 60 * 60 * 1000).toISOString();
    expect(formatRelativeTime(iso)).toBe('3 小时前');
  });

  it('returns "天前" for days ago', () => {
    const iso = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString();
    expect(formatRelativeTime(iso)).toBe('7 天前');
  });

  it('falls back to formatDate for > 30 days', () => {
    const iso = new Date(Date.now() - 60 * 24 * 60 * 60 * 1000).toISOString();
    const result = formatRelativeTime(iso);
    expect(result).toMatch(/2024/);
  });

  it('returns "-" for invalid string', () => {
    expect(formatRelativeTime('bad')).toBe('-');
  });
});

describe('formatPercent', () => {
  it('formats normal value', () => {
    expect(formatPercent(0.856)).toBe('85.6%');
  });

  it('returns "0%" for NaN', () => {
    expect(formatPercent(NaN)).toBe('0%');
  });

  it('supports custom decimals', () => {
    expect(formatPercent(0.8567, 2)).toBe('85.67%');
  });
});

describe('formatResponseTime', () => {
  it('formats ms < 1000', () => {
    expect(formatResponseTime(450)).toBe('450ms');
  });

  it('formats ms >= 1000 as seconds', () => {
    expect(formatResponseTime(2500)).toBe('2.5s');
  });
});

describe('formatNumber', () => {
  it('formats with locale separators', () => {
    const result = formatNumber(1234567);
    expect(result).toMatch(/1.*234.*567/);
  });
});

describe('truncate', () => {
  it('returns text within limit unchanged', () => {
    expect(truncate('hello', 10)).toBe('hello');
  });

  it('truncates and adds "..." when exceeding limit', () => {
    expect(truncate('hello world', 5)).toBe('hello...');
  });
});
