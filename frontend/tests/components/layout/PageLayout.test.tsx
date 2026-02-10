import { describe, it, expect, vi } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/stores/auth', () => ({
  authStore: {
    isAuthenticated: vi.fn(() => false),
    init: vi.fn(),
    loading: vi.fn(() => false),
  },
}));

vi.mock('@/stores/theme', () => ({
  themeStore: {
    mode: vi.fn(() => 'light'),
    effective: vi.fn(() => 'light'),
    toggle: vi.fn(),
  },
}));

vi.mock('@/api/client', () => ({
  unauthorized: vi.fn(() => false),
  resetUnauthorized: vi.fn(),
}));

describe('PageLayout', () => {
  it('renders Navigation and children', async () => {
    const { PageLayout } = await import('@/components/layout/PageLayout');
    renderWithProviders(() => <PageLayout>Test Content</PageLayout>);
    expect(screen.getByText('WordMaster')).toBeInTheDocument();
    expect(screen.getByText('Test Content')).toBeInTheDocument();
  });

  it('renders footer', async () => {
    const { PageLayout } = await import('@/components/layout/PageLayout');
    renderWithProviders(() => <PageLayout>X</PageLayout>);
    expect(screen.getByText(/WordMaster - 智能英语词汇学习平台/)).toBeInTheDocument();
  });
});
