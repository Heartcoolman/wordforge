import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { WorkerGridCell } from '@/components/ui/WorkerGridCell';

describe('WorkerGridCell', () => {
  it('renders name', () => {
    render(() => <WorkerGridCell name="amas_decision_loop" />);
    expect(screen.getByText('amas_decision_loop')).toBeInTheDocument();
  });

  it('shows relative time for lastRunAt', () => {
    const oneMinuteAgo = Date.now() - 60_000;
    render(() => <WorkerGridCell name="w" lastRunAt={oneMinuteAgo} outcome="ok" />);
    expect(screen.getByText(/[12]m 前/)).toBeInTheDocument();
  });

  it('renders lastDurationMs as mono number', () => {
    render(() => <WorkerGridCell name="w" lastDurationMs={42} outcome="ok" />);
    expect(screen.getByText('42ms')).toBeInTheDocument();
  });

  it('shows error message on error outcome', () => {
    render(() => <WorkerGridCell name="w" outcome="error" lastError="conn refused" />);
    expect(screen.getByText('conn refused')).toBeInTheDocument();
  });

  it('uses div by default, button when onClick provided', () => {
    const handler = vi.fn();
    const { container: a } = render(() => <WorkerGridCell name="w" />);
    expect(a.querySelector('div')).toBeTruthy();

    const { container: b } = render(() => <WorkerGridCell name="w" onClick={handler} />);
    const btn = b.querySelector('button')!;
    expect(btn).toBeTruthy();
    fireEvent.click(btn);
    expect(handler).toHaveBeenCalled();
  });
});
