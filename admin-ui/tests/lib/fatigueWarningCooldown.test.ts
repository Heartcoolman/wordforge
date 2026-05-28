import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

describe('checkFatigueWarningCooldown', () => {
  beforeEach(() => {
    sessionStorage.clear();
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-18T00:00:00Z'));
    vi.resetModules();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('first call returns true and persists timestamp', async () => {
    const { checkFatigueWarningCooldown } = await import('@/lib/fatigueWarningCooldown');
    expect(checkFatigueWarningCooldown()).toBe(true);
    expect(sessionStorage.getItem('lastFatigueWarningTime')).toBe(String(Date.now()));
  });

  it('second call within cooldown window returns false', async () => {
    const { checkFatigueWarningCooldown } = await import('@/lib/fatigueWarningCooldown');
    expect(checkFatigueWarningCooldown()).toBe(true);
    vi.advanceTimersByTime(1000);
    expect(checkFatigueWarningCooldown()).toBe(false);
  });

  it('returns true again after cooldown elapses', async () => {
    const { FATIGUE_WARNING_COOLDOWN_MS } = await import('@/lib/constants');
    const { checkFatigueWarningCooldown } = await import('@/lib/fatigueWarningCooldown');
    expect(checkFatigueWarningCooldown()).toBe(true);
    vi.advanceTimersByTime(FATIGUE_WARNING_COOLDOWN_MS + 1);
    expect(checkFatigueWarningCooldown()).toBe(true);
  });

  it('honors a previously persisted timestamp on module init', async () => {
    const now = Date.now();
    sessionStorage.setItem('lastFatigueWarningTime', String(now));
    const { checkFatigueWarningCooldown } = await import('@/lib/fatigueWarningCooldown');
    expect(checkFatigueWarningCooldown()).toBe(false);
  });
});
