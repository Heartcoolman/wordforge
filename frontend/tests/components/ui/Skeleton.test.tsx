import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { Skeleton, CardSkeleton } from '@/components/ui/Skeleton';

describe('Skeleton', () => {
  it('renders with default height', () => {
    const { container } = render(() => <Skeleton />);
    const el = container.firstElementChild as HTMLElement;
    expect(el.style.height).toBe('1rem');
  });

  it('applies rounded-full when rounded=true', () => {
    const { container } = render(() => <Skeleton rounded />);
    expect(container.firstElementChild!.className).toContain('rounded-full');
  });
});

describe('CardSkeleton', () => {
  it('renders skeleton elements', () => {
    const { container } = render(() => <CardSkeleton />);
    expect(container.querySelectorAll('.animate-pulse').length).toBeGreaterThanOrEqual(3);
  });
});
