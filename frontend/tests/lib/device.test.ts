import { afterEach, describe, expect, it, vi } from 'vitest';

import { getDevicePlatform } from '@/lib/device';

function setNavigator(options: { userAgent: string; maxTouchPoints?: number }) {
  Object.defineProperty(navigator, 'userAgent', {
    configurable: true,
    value: options.userAgent,
  });
  Object.defineProperty(navigator, 'maxTouchPoints', {
    configurable: true,
    value: options.maxTouchPoints ?? 0,
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('getDevicePlatform', () => {
  it('uses Capacitor platform when available', () => {
    vi.stubGlobal('Capacitor', { getPlatform: () => 'ios' });
    setNavigator({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)' });

    expect(getDevicePlatform()).toBe('ios');
  });

  it('detects iPhone user agents as ios', () => {
    setNavigator({
      userAgent:
        'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Version/17.0 Mobile/15E148 Safari/604.1',
    });

    expect(getDevicePlatform()).toBe('ios');
  });

  it('detects iPadOS desktop-style user agents as ios', () => {
    setNavigator({
      userAgent:
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/17.0 Safari/605.1.15',
      maxTouchPoints: 5,
    });

    expect(getDevicePlatform()).toBe('ios');
  });

  it('detects Android user agents as android', () => {
    setNavigator({
      userAgent:
        'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/120.0 Mobile Safari/537.36',
    });

    expect(getDevicePlatform()).toBe('android');
  });

  it('keeps desktop browsers as web', () => {
    setNavigator({
      userAgent:
        'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 Chrome/120.0 Safari/537.36',
    });

    expect(getDevicePlatform()).toBe('web');
  });
});
