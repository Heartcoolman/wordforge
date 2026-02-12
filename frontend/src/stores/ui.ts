import { createSignal, createRoot } from 'solid-js';
import { TOAST_DURATION_MS, TOAST_ERROR_DURATION_MS } from '@/lib/constants';

export interface ToastItem {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  title: string;
  message?: string;
  duration?: number;
}

function createUiStore() {
  const [sidebarOpen, setSidebarOpen] = createSignal(false);
  const [toasts, setToasts] = createSignal<ToastItem[]>([]);

  let toastCounter = 0;

  function addToast(toast: Omit<ToastItem, 'id'>) {
    toastCounter = (toastCounter + 1) % Number.MAX_SAFE_INTEGER;
    const id = `toast-${toastCounter}`;
    const item: ToastItem = { ...toast, id };
    setToasts((prev) => [...prev, item]);

    const duration = toast.duration ?? TOAST_DURATION_MS;
    setTimeout(() => removeToast(id), duration);
    return id;
  }

  function removeToast(id: string) {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }

  // Convenience methods
  const toast = {
    success: (title: string, message?: string) => addToast({ type: 'success', title, message }),
    error: (title: string, message?: string) => addToast({ type: 'error', title, message, duration: TOAST_ERROR_DURATION_MS }),
    warning: (title: string, message?: string) => addToast({ type: 'warning', title, message }),
    info: (title: string, message?: string) => addToast({ type: 'info', title, message }),
  };

  function toggleSidebar() {
    setSidebarOpen((prev) => !prev);
  }

  return {
    sidebarOpen,
    setSidebarOpen,
    toggleSidebar,
    toasts,
    addToast,
    removeToast,
    toast,
  };
}

export const uiStore = createRoot(createUiStore);
