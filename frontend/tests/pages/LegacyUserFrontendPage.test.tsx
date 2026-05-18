import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

describe('LegacyUserFrontendPage', () => {
  const originalEnv = { ...import.meta.env };

  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    Object.assign(import.meta.env, originalEnv);
  });

  async function renderPage() {
    const mod = await import('@/pages/LegacyUserFrontendPage');
    return renderWithProviders(() => <mod.default />);
  }

  it('shows heading and admin link without env URL', async () => {
    (import.meta.env as Record<string, unknown>).VITE_USER_APP_URL = undefined;
    await renderPage();
    expect(screen.getByText('用户前端已迁移到独立仓库')).toBeInTheDocument();
    expect(screen.getByText('打开管理后台')).toBeInTheDocument();
    expect(screen.getByText(/未配置/)).toBeInTheDocument();
  });

  it('renders external link when VITE_USER_APP_URL is set', async () => {
    (import.meta.env as Record<string, unknown>).VITE_USER_APP_URL = 'https://example.com/';
    await renderPage();
    const link = screen.getByText('前往用户前端').closest('a') as HTMLAnchorElement;
    expect(link).toBeTruthy();
    expect(link.href).toContain('example.com');
  });

  it('falls back to env url string when URL parsing throws', async () => {
    // 非法 base：'not a url' 让 new URL(path, base) 抛 TypeError，触发 catch 分支
    (import.meta.env as Record<string, unknown>).VITE_USER_APP_URL = 'not a url';
    await renderPage();
    const link = screen.getByText('前往用户前端').closest('a') as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe('not a url');
  });
});
