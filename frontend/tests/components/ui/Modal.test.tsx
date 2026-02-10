import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Modal } from '@/components/ui/Modal';

describe('Modal', () => {
  it('not rendered when open=false', () => {
    render(() => <Modal open={false} onClose={() => {}}>Content</Modal>);
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('rendered via Portal when open=true', () => {
    render(() => <Modal open={true} onClose={() => {}}>Content</Modal>);
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('shows title', () => {
    render(() => <Modal open={true} onClose={() => {}} title="My Modal">Body</Modal>);
    expect(screen.getByText('My Modal')).toBeInTheDocument();
  });

  it('calls onClose on backdrop click', async () => {
    const onClose = vi.fn();
    render(() => <Modal open={true} onClose={onClose}>Body</Modal>);
    const backdrop = screen.getByRole('dialog').parentElement!.querySelector('.absolute.inset-0') as HTMLElement;
    await fireEvent.click(backdrop);
    expect(onClose).toHaveBeenCalled();
  });

  it('calls onClose on Escape key', async () => {
    const onClose = vi.fn();
    render(() => <Modal open={true} onClose={onClose}>Body</Modal>);
    await fireEvent.keyDown(document, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('sets body overflow hidden when open', () => {
    render(() => <Modal open={true} onClose={() => {}}>Body</Modal>);
    expect(document.body.style.overflow).toBe('hidden');
  });

  it('has role="dialog" and aria-modal', () => {
    render(() => <Modal open={true} onClose={() => {}}>Body</Modal>);
    const dialog = screen.getByRole('dialog');
    expect(dialog).toHaveAttribute('aria-modal', 'true');
  });
});
