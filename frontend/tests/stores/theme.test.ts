import { describe, it, expect, vi, beforeEach } from 'vitest';

const THEME_KEY = 'theme';

async function getFreshStore() {
  vi.resetModules();
  const mod = await import('@/stores/theme');
  return mod.themeStore;
}

describe('themeStore', () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.classList.remove('dark');
  });

  it('mode defaults to system when no localStorage', async () => {
    const store = await getFreshStore();
    expect(store.mode()).toBe('system');
  });

  it('mode reads from localStorage on init', async () => {
    localStorage.setItem(THEME_KEY, 'dark');
    const store = await getFreshStore();
    expect(store.mode()).toBe('dark');
  });

  it('setMode changes mode', async () => {
    const store = await getFreshStore();
    store.setMode('dark');
    expect(store.mode()).toBe('dark');
  });

  it('effective resolves system to light when matchMedia matches false', async () => {
    // setup.ts mocks matchMedia with matches: false (light)
    const store = await getFreshStore();
    store.setMode('system');
    expect(store.effective()).toBe('light');
  });

  it('toggle cycles light -> dark -> system -> light', async () => {
    localStorage.setItem(THEME_KEY, 'light');
    const store = await getFreshStore();
    expect(store.mode()).toBe('light');
    store.toggle();
    expect(store.mode()).toBe('dark');
    store.toggle();
    expect(store.mode()).toBe('system');
    store.toggle();
    expect(store.mode()).toBe('light');
  });

  it('applyTheme adds dark class for dark mode', async () => {
    const store = await getFreshStore();
    store.setMode('dark');
    expect(document.documentElement.classList.contains('dark')).toBe(true);
  });

  it('applyTheme removes dark class for light mode', async () => {
    document.documentElement.classList.add('dark');
    const store = await getFreshStore();
    store.setMode('light');
    expect(document.documentElement.classList.contains('dark')).toBe(false);
  });

  it('mode persists to localStorage with raw theme key', async () => {
    const store = await getFreshStore();
    store.setMode('dark');
    expect(localStorage.getItem(THEME_KEY)).toBe('dark');
  });
});
