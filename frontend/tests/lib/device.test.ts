import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { getDevicePlatform, getDeviceId, generateUUID, collectDeviceFingerprint } from '@/lib/device';

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

  it('ignores Capacitor platforms that are not ios/android', () => {
    vi.stubGlobal('Capacitor', { getPlatform: () => 'electron' });
    setNavigator({ userAgent: 'Mozilla/5.0 (Windows NT 10.0)' });
    // 因为不是 ios/android，应该走 UA fallback
    expect(getDevicePlatform()).toBe('web');
  });
});

describe('generateUUID', () => {
  beforeEach(() => {
    // 重置 crypto stubs
    vi.unstubAllGlobals();
  });

  it('uses crypto.randomUUID when available', () => {
    const id = generateUUID();
    expect(id).toMatch(/^[0-9a-f-]+$/i);
  });

  it('falls back to manual UUID generation when randomUUID is missing', () => {
    vi.stubGlobal('crypto', {
      // 故意不提供 randomUUID
      getRandomValues: (arr: Uint8Array) => {
        for (let i = 0; i < arr.length; i++) arr[i] = i;
        return arr;
      },
    });
    const id = generateUUID();
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  });
});

describe('getDeviceId', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('generates and persists a new device id on first call', () => {
    const id1 = getDeviceId();
    expect(id1).toBeTruthy();
    expect(localStorage.getItem('eng_device_id')).toBe(id1);
  });

  it('returns the existing stored device id on subsequent calls', () => {
    localStorage.setItem('eng_device_id', 'preset-id');
    expect(getDeviceId()).toBe('preset-id');
  });
});

describe('collectDeviceFingerprint', () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns a fingerprint with required keys parsed from navigator/screen', () => {
    const fp = collectDeviceFingerprint();
    expect(fp).toHaveProperty('cpuCores');
    expect(fp).toHaveProperty('screenWidth');
    expect(fp).toHaveProperty('screenHeight');
    expect(fp).toHaveProperty('osName');
    expect(fp).toHaveProperty('browserName');
    expect(typeof fp.timezone).toBe('string');
    expect(typeof fp.touchSupport).toBe('boolean');
  });

  it('detects iOS / Safari user agents in fingerprint', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 Version/17.2 Mobile/15E148 Safari/604.1',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName.startsWith('iOS')).toBe(true);
    expect(fp.browserName).toBe('Safari');
  });

  it('detects macOS Chrome', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName.startsWith('macOS')).toBe(true);
    expect(fp.browserName).toBe('Chrome');
  });

  it('detects Android / Firefox', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Android 14; Mobile; rv:121.0) Gecko/121.0 Firefox/121.0',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName.startsWith('Android')).toBe(true);
    expect(fp.browserName).toBe('Firefox');
  });

  it('detects Windows / Edge', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName.startsWith('Windows')).toBe(true);
    expect(fp.browserName).toBe('Edge');
  });

  it('detects Linux / Opera', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 OPR/106.0.0.0',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName).toBe('Linux');
    expect(fp.browserName).toBe('Opera');
  });

  it('falls back to Unknown when UA does not match any platform/browser', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Unknown/0.0',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName).toBe('Unknown');
    expect(fp.browserName).toBe('Unknown');
  });

  it('falls back to empty version string when OS regex does not match', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'iPhone-no-version-string-without-os-number',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName).toBe('iOS ');
  });

  it('parses Mac OS X without minor version when regex misses', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Something Mac OS X without-number',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName).toBe('macOS ');
  });

  it('parses Android UA without version-number gracefully', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Android; Mobile)',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName).toBe('Android ');
  });

  it('parses Windows UA without version regex match', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Windows; lite)',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.osName).toBe('Windows ');
  });

  it('parses Edge/Chrome/Firefox/Safari/Opera versions falling back to empty when regex misses', () => {
    const cases = [
      { ua: 'Edg/', name: 'Edge' },
      { ua: 'OPR/', name: 'Opera' },
      { ua: 'Chrome/', name: 'Chrome' },
      { ua: 'Firefox/', name: 'Firefox' },
      { ua: 'Safari/', name: 'Safari' },
    ];
    for (const c of cases) {
      Object.defineProperty(navigator, 'userAgent', { configurable: true, value: c.ua });
      const fp = collectDeviceFingerprint();
      expect(fp.browserName).toBe(c.name);
      expect(fp.browserVersion).toBe('');
    }
  });

  it('reports deviceMemory value when present on navigator', () => {
    const navAny = navigator as any;
    const original = navAny.deviceMemory;
    Object.defineProperty(navigator, 'deviceMemory', {
      configurable: true,
      value: 8,
    });
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)',
    });
    const fp = collectDeviceFingerprint();
    expect(fp.memoryGb).toBe(8);
    if (original === undefined) {
      delete navAny.deviceMemory;
    } else {
      Object.defineProperty(navigator, 'deviceMemory', { configurable: true, value: original });
    }
  });

  it('falls back to memoryGb=null when navigator.deviceMemory missing', () => {
    Object.defineProperty(navigator, 'userAgent', {
      configurable: true,
      value: 'Mozilla/5.0 (X11; Linux)',
    });
    // deviceMemory 不存在 → null fallback (用 any cast 来读取/删除)
    const navAny = navigator as any;
    const original = navAny.deviceMemory;
    delete navAny.deviceMemory;
    const fp = collectDeviceFingerprint();
    expect(fp.memoryGb).toBeNull();
    if (original !== undefined) navAny.deviceMemory = original;
  });
});
