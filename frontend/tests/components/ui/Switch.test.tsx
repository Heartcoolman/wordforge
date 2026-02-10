import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Switch } from '@/components/ui/Switch';

describe('Switch', () => {
  it('renders with role="switch"', () => {
    render(() => <Switch checked={false} onChange={() => {}} />);
    expect(screen.getByRole('switch')).toBeInTheDocument();
  });

  it('shows label', () => {
    render(() => <Switch checked={false} onChange={() => {}} label="Enable" />);
    expect(screen.getByText('Enable')).toBeInTheDocument();
  });

  it('calls onChange on click', async () => {
    const onChange = vi.fn();
    render(() => <Switch checked={false} onChange={onChange} />);
    await fireEvent.click(screen.getByRole('switch'));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it('sets aria-checked', () => {
    render(() => <Switch checked={true} onChange={() => {}} />);
    expect(screen.getByRole('switch')).toHaveAttribute('aria-checked', 'true');
  });
});
