import { createSignal, createEffect, createRoot } from 'solid-js';

export type ThemeMode = 'light' | 'dark' | 'system';

function createThemeStore() {
  // Use raw 'theme' key (not prefixed) to match index.html FOUC prevention script
  const THEME_KEY = 'theme';
  const stored = (() => {
    try { return localStorage.getItem(THEME_KEY) as ThemeMode | null; } catch { return null; }
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
    try { localStorage.setItem(THEME_KEY, m); } catch { /* storage unavailable */ }
    applyTheme(getEffective(m));
  });

  // Listen for system theme changes
  if (typeof window !== 'undefined') {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (mode() === 'system') {
        applyTheme(getEffective('system'));
      }
    });
  }

  function toggle() {
    const current = mode();
    if (current === 'light') setMode('dark');
    else if (current === 'dark') setMode('system');
    else setMode('light');
  }

  return { mode, setMode, effective, toggle };
}

export const themeStore = createRoot(createThemeStore);
