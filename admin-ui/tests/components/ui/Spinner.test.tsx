import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Spinner } from '@/components/ui/Spinner';

describe('Spinner', () => {
  it('renders SVG with role="status"', () => {
    render(() => <Spinner />);
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('applies size class', () => {
    render(() => <Spinner size="lg" />);
    const svg = screen.getByRole('status');
    expect(svg.classList.toString()).toContain('w-8');
  });

  it('has aria-label', () => {
    render(() => <Spinner />);
    expect(screen.getByRole('status')).toHaveAttribute('aria-label', '加载中');
  });
});
