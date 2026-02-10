import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Button } from '@/components/ui/Button';

describe('Button', () => {
  it('renders children text', () => {
    render(() => <Button>Click me</Button>);
    expect(screen.getByText('Click me')).toBeInTheDocument();
  });

  it('applies variant classes', () => {
    render(() => <Button variant="danger">Delete</Button>);
    const btn = screen.getByText('Delete');
    expect(btn.className).toContain('bg-error');
  });

  it('shows loading spinner when loading=true', () => {
    render(() => <Button loading>Save</Button>);
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('disabled when loading', () => {
    render(() => <Button loading>Save</Button>);
    expect(screen.getByRole('button')).toBeDisabled();
  });

  it('renders icon', () => {
    render(() => <Button icon={<span data-testid="icon">I</span>}>Go</Button>);
    expect(screen.getByTestId('icon')).toBeInTheDocument();
  });

  it('applies fullWidth class', () => {
    render(() => <Button fullWidth>Wide</Button>);
    expect(screen.getByRole('button').className).toContain('w-full');
  });
});
