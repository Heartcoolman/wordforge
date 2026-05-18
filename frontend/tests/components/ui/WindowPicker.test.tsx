import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { WindowPicker } from '@/components/ui/WindowPicker';

describe('WindowPicker', () => {
  it('renders default 7/14/30 options', () => {
    const [val] = createSignal(7);
    render(() => <WindowPicker value={val} onChange={() => {}} />);
    expect(screen.getByText('7天')).toBeInTheDocument();
    expect(screen.getByText('14天')).toBeInTheDocument();
    expect(screen.getByText('30天')).toBeInTheDocument();
  });

  it('marks active option with aria-pressed=true', () => {
    const [val] = createSignal(14);
    render(() => <WindowPicker value={val} onChange={() => {}} />);
    expect(screen.getByText('14天')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText('7天')).toHaveAttribute('aria-pressed', 'false');
  });

  it('calls onChange when option clicked', () => {
    const [val] = createSignal(7);
    const onChange = vi.fn();
    render(() => <WindowPicker value={val} onChange={onChange} />);
    fireEvent.click(screen.getByText('30天'));
    expect(onChange).toHaveBeenCalledWith(30);
  });

  it('uses custom options when provided', () => {
    const [val] = createSignal(60);
    render(() => <WindowPicker value={val} onChange={() => {}} options={[60, 90]} />);
    expect(screen.getByText('60天')).toBeInTheDocument();
    expect(screen.getByText('90天')).toBeInTheDocument();
  });
});
