import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { SystemLockedModal } from '@/components/SystemLockedModal';

describe('SystemLockedModal', () => {
  it('renders title and body copy', () => {
    render(() => <SystemLockedModal />);
    expect(screen.getByText('数据损坏')).toBeInTheDocument();
    expect(screen.getByText(/客户端数据已损坏/)).toBeInTheDocument();
  });

  it('stops click propagation on overlay', () => {
    const outerHandler = vi.fn();
    render(() => (
      <div onClick={outerHandler}>
        <SystemLockedModal />
      </div>
    ));
    const overlay = document.querySelector('[tabindex="0"]') as HTMLDivElement;
    expect(overlay).toBeTruthy();
    fireEvent.click(overlay);
    expect(outerHandler).not.toHaveBeenCalled();
  });

  it('prevents default on keydown', () => {
    render(() => <SystemLockedModal />);
    const overlay = document.querySelector('[tabindex="0"]') as HTMLDivElement;
    const result = fireEvent.keyDown(overlay, { key: 'Tab' });
    expect(result).toBe(false);
  });

  it('focuses the overlay on mount', () => {
    render(() => <SystemLockedModal />);
    const overlay = document.querySelector('[tabindex="0"]') as HTMLDivElement;
    expect(document.activeElement).toBe(overlay);
  });
});
