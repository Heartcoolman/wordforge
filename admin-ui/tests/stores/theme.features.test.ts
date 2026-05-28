import { describe, it, expect, vi, beforeEach } from 'vitest';

// 让 matchMedia 暴露 listener，能手动触发
let storedListener: ((e: MediaQueryListEvent) => void) | null = null;
const mediaQuery: Partial<MediaQueryList> = {
  matches: false,
  addEventListener: vi.fn((_: 'change', fn: any) => { storedListener = fn; }),
  removeEventListener: vi.fn(),
};

beforeEach(() => {
  storedListener = null;
  vi.resetModules();
  localStorage.clear();
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: vi.fn(() => mediaQuery as MediaQueryList),
  });
});

describe('themeStore media listener / cleanup', () => {
  it('reacts to system theme change when mode=system', async () => {
    const { themeStore } = await import('@/stores/theme');
    themeStore.setMode('system');
    (mediaQuery as { matches: boolean }).matches = true;
    if (storedListener) storedListener({ matches: true } as MediaQueryListEvent);
    expect(themeStore.effective()).toBe('dark');
  });

  it('does NOT react when mode=light explicitly', async () => {
    const { themeStore } = await import('@/stores/theme');
    themeStore.setMode('light');
    if (storedListener) storedListener({ matches: true } as MediaQueryListEvent);
    expect(themeStore.effective()).toBe('light');
  });

  it('cleanup removes the registered media listener', async () => {
    const { themeStore } = await import('@/stores/theme');
    themeStore.cleanup();
    expect(mediaQuery.removeEventListener).toHaveBeenCalled();
  });
});
