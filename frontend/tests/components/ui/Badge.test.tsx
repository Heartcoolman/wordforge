import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Badge } from '@/components/ui/Badge';

describe('Badge', () => {
  it('renders children', () => {
    render(() => <Badge>Active</Badge>);
    expect(screen.getByText('Active')).toBeInTheDocument();
  });

  it('applies variant classes', () => {
    render(() => <Badge variant="success">OK</Badge>);
    expect(screen.getByText('OK').className).toContain('text-success');
  });

  it('applies size', () => {
    render(() => <Badge size="sm">S</Badge>);
    expect(screen.getByText('S').className).toContain('text-[10px]');
  });
});
