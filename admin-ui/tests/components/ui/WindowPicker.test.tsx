import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { WindowPicker } from '@/components/ui/WindowPicker';

describe('WindowPicker', () => {
  it('renders default 7/14/30 options', () => {
    render(() => <WindowPicker value={7} onChange={() => {}} />);
    expect(screen.getByText('7天')).toBeInTheDocument();
    expect(screen.getByText('14天')).toBeInTheDocument();
    expect(screen.getByText('30天')).toBeInTheDocument();
  });

  it('marks active option with aria-checked=true', () => {
    render(() => <WindowPicker value={14} onChange={() => {}} />);
    expect(screen.getByText('14天')).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByText('7天')).toHaveAttribute('aria-checked', 'false');
  });

  it('calls onChange when option clicked', () => {
    const onChange = vi.fn();
    render(() => <WindowPicker value={7} onChange={onChange} />);
    fireEvent.click(screen.getByText('30天'));
    expect(onChange).toHaveBeenCalledWith(30);
  });

  it('uses custom options when provided', () => {
    render(() => <WindowPicker value={60} onChange={() => {}} options={[60, 90]} />);
    expect(screen.getByText('60天')).toBeInTheDocument();
    expect(screen.getByText('90天')).toBeInTheDocument();
  });

  it('exposes radiogroup semantics', () => {
    render(() => <WindowPicker value={7} onChange={() => {}} />);
    const group = screen.getByRole('radiogroup');
    expect(group).toHaveAttribute('aria-label', '时间范围选择');
    expect(group.querySelectorAll('[role="radio"]')).toHaveLength(3);
  });

  it('ArrowRight moves to next option', () => {
    const onChange = vi.fn();
    render(() => <WindowPicker value={7} onChange={onChange} />);
    const group = screen.getByRole('radiogroup');
    fireEvent.keyDown(group, { key: 'ArrowRight' });
    expect(onChange).toHaveBeenCalledWith(14);
  });

  it('ArrowLeft wraps to last option', () => {
    const onChange = vi.fn();
    render(() => <WindowPicker value={7} onChange={onChange} />);
    const group = screen.getByRole('radiogroup');
    fireEvent.keyDown(group, { key: 'ArrowLeft' });
    expect(onChange).toHaveBeenCalledWith(30);
  });

  it('Home/End jump to first/last', () => {
    const onChange = vi.fn();
    render(() => <WindowPicker value={14} onChange={onChange} />);
    const group = screen.getByRole('radiogroup');
    fireEvent.keyDown(group, { key: 'End' });
    expect(onChange).toHaveBeenCalledWith(30);
    fireEvent.keyDown(group, { key: 'Home' });
    expect(onChange).toHaveBeenCalledWith(7);
  });
});
