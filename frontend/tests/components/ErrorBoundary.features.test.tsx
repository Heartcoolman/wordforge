import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { createSignal, Show } from 'solid-js';
import { AppErrorBoundary } from '@/components/ErrorBoundary';

describe('AppErrorBoundary extra — reset & navigate handlers', () => {
  it('calls reset handler when 重试 clicked (recovers when child stops throwing)', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const [shouldThrow, setShouldThrow] = createSignal(true);
    function MaybeThrow() {
      return <Show when={!shouldThrow()} fallback={(() => { throw new Error('boom'); })()}>
        <p>safe-now</p>
      </Show>;
    }
    render(() => <AppErrorBoundary><MaybeThrow /></AppErrorBoundary>);
    expect(screen.getByText('重试')).toBeInTheDocument();
    setShouldThrow(false);
    fireEvent.click(screen.getByText('重试'));
    // reset 执行后应恢复显示
    expect(screen.queryByText('boom')).not.toBeInTheDocument();
    spy.mockRestore();
  });

  it('返回首页 button updates window.location.href', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const original = window.location.href;
    function Throwing(): never { throw new Error('nav-err'); }
    render(() => <AppErrorBoundary><Throwing /></AppErrorBoundary>);
    fireEvent.click(screen.getByText('返回首页'));
    expect(window.location.href).toBe('/');
    // 恢复，避免污染后续测试
    window.location.href = original;
    spy.mockRestore();
  });
});
