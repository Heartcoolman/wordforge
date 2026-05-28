import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { uiStore } from '@/stores/ui';
import { Toaster } from '@/components/ui/Toast';

describe('Toaster extra — close handler', () => {
  beforeEach(() => {
    // 清空所有遗留 toast
    while (uiStore.toasts().length) uiStore.removeToast(uiStore.toasts()[0].id);
  });

  it('clicking close removes the toast (after exit animation)', () => {
    vi.useFakeTimers();
    try {
      uiStore.addToast({ type: 'error', title: 'Err', duration: 60000 });
      render(() => <Toaster />);
      expect(uiStore.toasts().length).toBe(1);
      fireEvent.click(screen.getByLabelText('关闭'));
      // 点击触发 leaving 信号，出场动画结束后 (~220ms) 才从 store 剔除
      vi.advanceTimersByTime(300);
      expect(uiStore.toasts().length).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it('renders all 4 toast types (success/error/warning/info icons)', () => {
    uiStore.addToast({ type: 'success', title: 's', duration: 60000 });
    uiStore.addToast({ type: 'error', title: 'e', duration: 60000 });
    uiStore.addToast({ type: 'warning', title: 'w', duration: 60000 });
    uiStore.addToast({ type: 'info', title: 'i', duration: 60000 });
    render(() => <Toaster />);
    expect(screen.getByText('s')).toBeInTheDocument();
    expect(screen.getByText('e')).toBeInTheDocument();
    expect(screen.getByText('w')).toBeInTheDocument();
    expect(screen.getByText('i')).toBeInTheDocument();
  });
});
