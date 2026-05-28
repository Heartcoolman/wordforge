import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { Kbd } from '@/components/ui/Kbd';

describe('Kbd', () => {
  it('renders children inside <kbd>', () => {
    render(() => <Kbd>⌘K</Kbd>);
    const el = screen.getByText('⌘K');
    expect(el.tagName).toBe('KBD');
  });

  it('applies sm sizing', () => {
    render(() => <Kbd size="sm">ESC</Kbd>);
    const el = screen.getByText('ESC');
    expect(el.className).toContain('h-4');
  });

  it('applies md sizing by default', () => {
    render(() => <Kbd>Enter</Kbd>);
    const el = screen.getByText('Enter');
    expect(el.className).toContain('h-[18px]');
  });
});
