import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Card } from '@/components/ui/Card';

describe('Card', () => {
  it('renders children', () => {
    render(() => <Card>Hello</Card>);
    expect(screen.getByText('Hello')).toBeInTheDocument();
  });

  it('applies variant classes', () => {
    const { container } = render(() => <Card variant="outlined">Content</Card>);
    expect(container.firstElementChild!.className).toContain('border');
  });

  it('applies padding', () => {
    const { container } = render(() => <Card padding="lg">Content</Card>);
    expect(container.firstElementChild!.className).toContain('p-6');
  });

  it('applies hover effect', () => {
    const { container } = render(() => <Card hover>Content</Card>);
    expect(container.firstElementChild!.className).toContain('hover:shadow-lg');
  });
});
