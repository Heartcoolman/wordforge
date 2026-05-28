import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createRoot } from 'solid-js';
import { uiStore } from '@/stores/ui';

describe('uiStore', () => {
  beforeEach(() => {
    // Clean up toasts
    for (const t of uiStore.toasts()) {
      uiStore.removeToast(t.id);
    }
  });

  it('addToast adds a toast item', () => {
    createRoot((dispose) => {
      uiStore.addToast({ type: 'info', title: 'Test' });
      expect(uiStore.toasts()).toHaveLength(1);
      expect(uiStore.toasts()[0].title).toBe('Test');
      dispose();
    });
  });

  it('addToast auto-assigns id', () => {
    createRoot((dispose) => {
      const id = uiStore.addToast({ type: 'info', title: 'A' });
      expect(id).toMatch(/^toast-\d+$/);
      dispose();
    });
  });

  it('removeToast removes by id', () => {
    createRoot((dispose) => {
      const id = uiStore.addToast({ type: 'info', title: 'Remove me' });
      expect(uiStore.toasts()).toHaveLength(1);
      uiStore.removeToast(id);
      expect(uiStore.toasts()).toHaveLength(0);
      dispose();
    });
  });

  it('toast.success creates success toast', () => {
    createRoot((dispose) => {
      uiStore.toast.success('Done', 'All good');
      const t = uiStore.toasts().at(-1)!;
      expect(t.type).toBe('success');
      expect(t.title).toBe('Done');
      expect(t.message).toBe('All good');
      dispose();
    });
  });

  it('toast.error creates error toast with 6000ms duration', () => {
    vi.useFakeTimers();
    createRoot((dispose) => {
      uiStore.toast.error('Oops');
      const t = uiStore.toasts().at(-1)!;
      expect(t.type).toBe('error');
      expect(t.duration).toBe(6000);

      // Should still exist after 5s
      vi.advanceTimersByTime(5000);
      expect(uiStore.toasts().some((x) => x.id === t.id)).toBe(true);

      // Should be removed after 6s
      vi.advanceTimersByTime(1500);
      expect(uiStore.toasts().some((x) => x.id === t.id)).toBe(false);
      dispose();
    });
    vi.useRealTimers();
  });

  it('toast.warning creates warning toast', () => {
    createRoot((dispose) => {
      uiStore.toast.warning('Careful');
      const t = uiStore.toasts().at(-1)!;
      expect(t.type).toBe('warning');
      expect(t.title).toBe('Careful');
      dispose();
    });
  });

  it('toast.info creates info toast', () => {
    createRoot((dispose) => {
      uiStore.toast.info('FYI');
      const t = uiStore.toasts().at(-1)!;
      expect(t.type).toBe('info');
      expect(t.title).toBe('FYI');
      dispose();
    });
  });
});
