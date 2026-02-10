import { createSignal, createRoot } from 'solid-js';

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
    const id = `toast-${++toastCounter}`;
    const item: ToastItem = { ...toast, id };
    setToasts((prev) => [...prev, item]);

    const duration = toast.duration ?? 4000;
    setTimeout(() => removeToast(id), duration);
    return id;
  }

  function removeToast(id: string) {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }

  // Convenience methods
  const toast = {
    success: (title: string, message?: string) => addToast({ type: 'success', title, message }),
    error: (title: string, message?: string) => addToast({ type: 'error', title, message, duration: 6000 }),
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
