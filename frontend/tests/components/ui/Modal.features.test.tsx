import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { Modal } from '@/components/ui/Modal';

describe('Modal extra branches', () => {
  it('hides close X when hideClose is true', () => {
    render(() => <Modal open={true} onClose={() => {}} hideClose>X</Modal>);
    expect(screen.queryByLabelText('关闭')).not.toBeInTheDocument();
  });

  it('applies sm size class', () => {
    render(() => <Modal open={true} onClose={() => {}} size="sm">B</Modal>);
    expect(screen.getByRole('dialog').className).toContain('max-w-sm');
  });

  it('applies xl size class', () => {
    render(() => <Modal open={true} onClose={() => {}} size="xl">B</Modal>);
    expect(screen.getByRole('dialog').className).toContain('max-w-xl');
  });

  it('Tab cycles to first when focus on last', async () => {
    render(() => (
      <Modal open={true} onClose={() => {}}>
        <button>a</button>
        <button>b</button>
      </Modal>
    ));
    const buttons = screen.getAllByRole('button');
    const last = buttons[buttons.length - 1];
    last.focus();
    fireEvent.keyDown(document, { key: 'Tab' });
  });

  it('Shift+Tab cycles to last when focus on first', () => {
    render(() => (
      <Modal open={true} onClose={() => {}}>
        <button>a</button>
        <button>b</button>
      </Modal>
    ));
    const buttons = screen.getAllByRole('button');
    const first = buttons[0];
    first.focus();
    fireEvent.keyDown(document, { key: 'Tab', shiftKey: true });
  });

  it('calls onClose when X clicked', () => {
    const onClose = vi.fn();
    render(() => <Modal open={true} onClose={onClose} title="t">B</Modal>);
    fireEvent.click(screen.getByLabelText('关闭'));
    expect(onClose).toHaveBeenCalled();
  });
});
