import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Tabs } from '@/components/ui/Tabs';

const tabs = [
  { id: 'a', label: 'Tab A' },
  { id: 'b', label: 'Tab B' },
];

describe('Tabs', () => {
  it('renders tab labels', () => {
    render(() => <Tabs tabs={tabs} active="a" onChange={() => {}} />);
    expect(screen.getByText('Tab A')).toBeInTheDocument();
    expect(screen.getByText('Tab B')).toBeInTheDocument();
  });

  it('highlights active tab', () => {
    render(() => <Tabs tabs={tabs} active="a" onChange={() => {}} />);
    expect(screen.getByText('Tab A').className).toContain('text-accent');
  });

  it('calls onChange on click', async () => {
    const onChange = vi.fn();
    render(() => <Tabs tabs={tabs} active="a" onChange={onChange} />);
    await fireEvent.click(screen.getByText('Tab B'));
    expect(onChange).toHaveBeenCalledWith('b');
  });
});
