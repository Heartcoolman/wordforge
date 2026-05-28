import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Tabs } from '@/components/ui/Tabs';

const tabs = [
  { id: 'a', label: 'A' },
  { id: 'b', label: 'B' },
  { id: 'c', label: 'C' },
];

describe('Tabs extra', () => {
  it('ArrowRight focuses next tab', () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="a" onChange={onChange} />);
    const tablist = document.querySelector('[role="tablist"]') as HTMLElement;
    fireEvent.keyDown(tablist, { key: 'ArrowRight' });
    expect(onChange).toHaveBeenCalledWith('b');
  });

  it('ArrowLeft wraps to last tab', () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="a" onChange={onChange} />);
    const tablist = document.querySelector('[role="tablist"]') as HTMLElement;
    fireEvent.keyDown(tablist, { key: 'ArrowLeft' });
    expect(onChange).toHaveBeenCalledWith('c');
  });

  it('ArrowRight wraps around at end', () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="c" onChange={onChange} />);
    const tablist = document.querySelector('[role="tablist"]') as HTMLElement;
    fireEvent.keyDown(tablist, { key: 'ArrowRight' });
    expect(onChange).toHaveBeenCalledWith('a');
  });

  it('Home jumps to first', () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="c" onChange={onChange} />);
    const tablist = document.querySelector('[role="tablist"]') as HTMLElement;
    fireEvent.keyDown(tablist, { key: 'Home' });
    expect(onChange).toHaveBeenCalledWith('a');
  });

  it('End jumps to last', () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="a" onChange={onChange} />);
    const tablist = document.querySelector('[role="tablist"]') as HTMLElement;
    fireEvent.keyDown(tablist, { key: 'End' });
    expect(onChange).toHaveBeenCalledWith('c');
  });

  it('manual mode: ArrowRight does NOT trigger onChange', () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="a" onChange={onChange} activation="manual" />);
    const tablist = document.querySelector('[role="tablist"]') as HTMLElement;
    fireEvent.keyDown(tablist, { key: 'ArrowRight' });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('manual mode: Enter on focused tab triggers onChange', () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="a" onChange={onChange} activation="manual" />);
    const tablist = document.querySelector('[role="tablist"]') as HTMLElement;
    fireEvent.keyDown(tablist, { key: 'ArrowRight' });
    fireEvent.keyDown(tablist, { key: 'Enter' });
    expect(onChange).toHaveBeenCalledWith('b');
  });

  it('tabs carry id and aria-controls', () => {
    render(() => <Tabs tabs={[{ id: 'foo', label: 'Foo' }]} active="foo" onChange={() => {}} />);
    const btn = screen.getByText('Foo');
    expect(btn.id).toBe('tab-foo');
    expect(btn.getAttribute('aria-controls')).toBe('tabpanel-foo');
  });

  it('renders icon when provided', () => {
    render(() => (
      <Tabs
        tabs={[{ id: 'x', label: 'X', icon: <span data-testid="icon">i</span> }]}
        active="x"
        onChange={() => {}}
      />
    ));
    expect(screen.getByTestId('icon')).toBeInTheDocument();
  });
});
