import { describe, it, expect } from 'vitest';
import {
  formatResponseTime,
  formatNumber,
  formatDuration,
  formatAccuracy,
} from '@/utils/formatters';

describe('formatters extra — branch coverage', () => {
  it('formatResponseTime: < 1000 ms branch', () => {
    expect(formatResponseTime(123)).toBe('123ms');
  });
  it('formatResponseTime: >= 1000 ms branch', () => {
    expect(formatResponseTime(2500)).toBe('2.5s');
  });
  it('formatResponseTime: negative / NaN returns "-"', () => {
    expect(formatResponseTime(-1)).toBe('-');
    expect(formatResponseTime(NaN)).toBe('-');
    expect(formatResponseTime(Infinity)).toBe('-');
  });

  it('formatNumber: finite + non-finite branches', () => {
    expect(formatNumber(12345)).toMatch(/12,345|12 345/);
    expect(formatNumber(NaN)).toBe('-');
    expect(formatNumber(Infinity)).toBe('-');
  });

  it('formatDuration: null/undefined → "-"', () => {
    expect(formatDuration(null)).toBe('-');
    expect(formatDuration(undefined)).toBe('-');
    expect(formatDuration(-1)).toBe('-');
    expect(formatDuration(NaN)).toBe('-');
  });
  it('formatDuration: hours branch', () => {
    expect(formatDuration(3700)).toBe('1h 1m');
  });
  it('formatDuration: minutes branch', () => {
    expect(formatDuration(65)).toBe('1m 5s');
  });
  it('formatDuration: seconds branch', () => {
    expect(formatDuration(5)).toBe('5s');
  });

  it('formatAccuracy: null + finite', () => {
    expect(formatAccuracy(null)).toBe('-');
    expect(formatAccuracy(undefined)).toBe('-');
    expect(formatAccuracy(Infinity)).toBe('-');
    expect(formatAccuracy(0.85)).toBe('85.0%');
    expect(formatAccuracy(0.852, 2)).toBe('85.20%');
  });
});
