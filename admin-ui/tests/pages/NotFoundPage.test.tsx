import { describe, it, expect } from 'vitest';
import { screen } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import NotFoundPage from '@/pages/NotFoundPage';

describe('NotFoundPage', () => {
  it('shows 404 text', () => {
    renderWithProviders(() => <NotFoundPage />);
    expect(screen.getByText('404')).toBeInTheDocument();
  });

  it('shows 返回首页 button', () => {
    renderWithProviders(() => <NotFoundPage />);
    const link = screen.getByText('返回首页').closest('a');
    expect(link).toHaveAttribute('href', '/');
  });
});
