import { createSignal, createEffect, createRoot } from 'solid-js';
import { storage, STORAGE_KEYS } from '@/lib/storage';

export type ThemeMode = 'light' | 'dark' | 'system';

function createThemeStore() {
  const stored = (() => {
    const v = storage.getString(STORAGE_KEYS.THEME, '');
    return (v === 'light' || v === 'dark' || v === 'system') ? v as ThemeMode : null;
  })();
  const [mode, setMode] = createSignal<ThemeMode>(stored ?? 'system');

  function getEffective(m: ThemeMode): 'light' | 'dark' {
    if (m === 'system') {
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    return m;
  }

  const [effective, setEffective] = createSignal<'light' | 'dark'>(getEffective(mode()));

  // Apply theme to <html>
  function applyTheme(theme: 'light' | 'dark') {
    if (theme === 'dark') {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
    setEffective(theme);
  }

  // React to mode changes
  createEffect(() => {
    const m = mode();
    storage.setString(STORAGE_KEYS.THEME, m);
    applyTheme(getEffective(m));
  });

  // Listen for system theme changes - 保存引用以便测试清理
  let mediaQuery: MediaQueryList | null = null;
  let mediaQueryListener: ((e: MediaQueryListEvent) => void) | null = null;

  if (typeof window !== 'undefined') {
    mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    mediaQueryListener = () => {
      if (mode() === 'system') {
        applyTheme(getEffective('system'));
      }
    };
    mediaQuery.addEventListener('change', mediaQueryListener);
  }

  /** 移除系统主题变化监听器（供测试清理使用） */
  function cleanup() {
    if (mediaQuery && mediaQueryListener) {
      mediaQuery.removeEventListener('change', mediaQueryListener);
    }
  }

  function toggle() {
    const current = mode();
    if (current === 'light') setMode('dark');
    else if (current === 'dark') setMode('system');
    else setMode('light');
  }

  return { mode, setMode, effective, toggle, cleanup };
}

export const themeStore = createRoot(createThemeStore);
