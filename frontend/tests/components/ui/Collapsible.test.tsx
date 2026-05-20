import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent, screen } from '@solidjs/testing-library';
import { Collapsible } from '@/components/ui/Collapsible';

describe('Collapsible', () => {
  it('defaults to collapsed', () => {
    render(() => (
      <Collapsible title="Beta 通道">
        <div data-testid="content">内容</div>
      </Collapsible>
    ));
    expect(screen.queryByTestId('content')).toBeNull();
  });

  it('toggles on click and reflects aria-expanded', () => {
    render(() => (
      <Collapsible title="Beta 通道">
        <div data-testid="content">内容</div>
      </Collapsible>
    ));
    const trigger = screen.getByRole('button', { name: /Beta 通道/ });
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(trigger);
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    expect(screen.queryByTestId('content')).not.toBeNull();
    fireEvent.click(trigger);
    expect(trigger.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByTestId('content')).toBeNull();
  });

  it('renders badge when provided', () => {
    render(() => (
      <Collapsible title="Beta 通道" badge="v0.6.0-beta.3">
        <div>x</div>
      </Collapsible>
    ));
    expect(screen.queryByText('v0.6.0-beta.3')).not.toBeNull();
  });

  it('does not render badge when null/undefined', () => {
    render(() => (
      <Collapsible title="Beta 通道" badge={null}>
        <div>x</div>
      </Collapsible>
    ));
    // badge span 应不存在；查找已知格式的版本字符串
    expect(screen.queryByText(/v\d+/)).toBeNull();
  });

  it('starts expanded with defaultOpen', () => {
    render(() => (
      <Collapsible title="X" defaultOpen>
        <div data-testid="content">内容</div>
      </Collapsible>
    ));
    expect(screen.queryByTestId('content')).not.toBeNull();
  });

  it('calls onToggle with the new open state', () => {
    const onToggle = vi.fn();
    render(() => (
      <Collapsible title="X" onToggle={onToggle}>
        <div>x</div>
      </Collapsible>
    ));
    const trigger = screen.getByRole('button', { name: /X/ });
    fireEvent.click(trigger);
    expect(onToggle).toHaveBeenCalledWith(true);
    fireEvent.click(trigger);
    expect(onToggle).toHaveBeenCalledWith(false);
  });
});
