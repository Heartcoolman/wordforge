import { describe, it, expect, vi } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/stores/auth', () => ({
  authStore: {
    isAuthenticated: vi.fn(() => false),
  },
}));

vi.mock('@/stores/theme', () => ({
  themeStore: {
    mode: vi.fn(() => 'light'),
    effective: vi.fn(() => 'light'),
    toggle: vi.fn(),
  },
}));

describe('Navigation', () => {
  it('renders brand link', async () => {
    const { Navigation } = await import('@/components/layout/Navigation');
    renderWithProviders(() => <Navigation />);
    expect(screen.getByText('WordMaster')).toBeInTheDocument();
  });

  it('shows login/register when not authenticated', async () => {
    const { Navigation } = await import('@/components/layout/Navigation');
    renderWithProviders(() => <Navigation />);
    expect(screen.getByText('登录')).toBeInTheDocument();
    expect(screen.getByText('注册')).toBeInTheDocument();
  });

  it('theme toggle button present', async () => {
    const { Navigation } = await import('@/components/layout/Navigation');
    renderWithProviders(() => <Navigation />);
    const { themeStore } = await import('@/stores/theme');
    const btn = screen.getByTitle(`Theme: ${themeStore.mode()}`);
    expect(btn).toBeInTheDocument();
  });
});
