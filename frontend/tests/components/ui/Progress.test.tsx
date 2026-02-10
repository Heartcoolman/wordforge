import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { ProgressBar, CircularProgress } from '@/components/ui/Progress';

describe('ProgressBar', () => {
  it('renders progress bar with correct width%', () => {
    const { container } = render(() => <ProgressBar value={50} />);
    const bar = container.querySelector('[style]') as HTMLElement;
    expect(bar.style.width).toBe('50%');
  });

  it('clamps value between 0-100', () => {
    const { container } = render(() => <ProgressBar value={150} />);
    const bar = container.querySelector('[style]') as HTMLElement;
    expect(bar.style.width).toBe('100%');
  });

  it('shows label when showLabel=true', () => {
    render(() => <ProgressBar value={30} showLabel />);
    expect(screen.getByText('30%')).toBeInTheDocument();
  });
});

describe('CircularProgress', () => {
  it('renders CircularProgress', () => {
    const { container } = render(() => <CircularProgress value={75} />);
    expect(container.querySelector('svg')).toBeInTheDocument();
    expect(screen.getByText('75%')).toBeInTheDocument();
  });
});
