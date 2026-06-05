import { describe, it, expect } from 'vitest';
import { render } from '@solidjs/testing-library';
import { Sparkline } from '@/components/ui/Sparkline';

describe('Sparkline', () => {
  it('renders nothing when data length < 2', () => {
    const { container } = render(() => <Sparkline data={[]} />);
    expect(container.querySelector('svg')).toBeNull();
    const { container: c2 } = render(() => <Sparkline data={[5]} />);
    expect(c2.querySelector('svg')).toBeNull();
  });

  it('renders SVG with line and area paths when data ≥ 2', () => {
    const { container } = render(() => <Sparkline data={[1, 2, 3, 4, 5]} />);
    const svg = container.querySelector('svg');
    expect(svg).not.toBeNull();
    const paths = svg!.querySelectorAll('path');
    expect(paths.length).toBe(2); // area + line
  });

  it('uses aria-label when provided, otherwise presentation', () => {
    const { container: a } = render(() => <Sparkline data={[1, 2]} ariaLabel="DAU" />);
    expect(a.querySelector('svg')?.getAttribute('aria-label')).toBe('DAU');
    expect(a.querySelector('svg')?.getAttribute('role')).toBe('img');

    const { container: b } = render(() => <Sparkline data={[1, 2]} />);
    expect(b.querySelector('svg')?.getAttribute('role')).toBe('presentation');
  });
});
