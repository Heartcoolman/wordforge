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
