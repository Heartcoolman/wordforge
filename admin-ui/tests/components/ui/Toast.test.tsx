import { describe, it, expect } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { uiStore } from '@/stores/ui';
import { Toaster } from '@/components/ui/Toast';

describe('Toaster', () => {
  it('renders via Portal', () => {
    render(() => <Toaster />);
    expect(document.querySelector('.fixed.top-4.right-4')).toBeInTheDocument();
  });

  it('shows toast title and message', () => {
    uiStore.addToast({ type: 'success', title: 'Saved', message: 'Item saved', duration: 60000 });
    render(() => <Toaster />);
    expect(screen.getByText('Saved')).toBeInTheDocument();
    expect(screen.getByText('Item saved')).toBeInTheDocument();
  });

  it('has close button', () => {
    uiStore.addToast({ type: 'info', title: 'Info', duration: 60000 });
    render(() => <Toaster />);
    expect(screen.getAllByLabelText('关闭').length).toBeGreaterThan(0);
  });
});
