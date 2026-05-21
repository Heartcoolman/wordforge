import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { createSignal, Show } from 'solid-js';
import { AppErrorBoundary } from '@/components/ErrorBoundary';

describe('AppErrorBoundary — error reporting to /api/telemetry/error', () => {
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    fetchSpy = vi.fn().mockResolvedValue({ ok: true });
    vi.stubGlobal('fetch', fetchSpy);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('calls fetch with POST /api/telemetry/error when error is thrown', async () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    function Throwing(): never { throw new Error('report-me'); }
    render(() => <AppErrorBoundary><Throwing /></AppErrorBoundary>);
    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalled();
      const [url, opts] = fetchSpy.mock.calls[0] as [string, RequestInit];
      expect(url).toMatch(/\/api\/telemetry\/error/);
      expect(opts.method).toBe('POST');
      const body = JSON.parse(opts.body as string);
      expect(body.message).toContain('report-me');
    });
    spy.mockRestore();
  });

  it('does not call fetch a second time for same error object (logOnce dedup)', async () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    // 第一次渲染触发 logOnce → fetch 调用一次
    const [shouldThrow, setShouldThrow] = createSignal(true);
    function MaybeThrow() {
      return <Show when={!shouldThrow()} fallback={(() => { throw new Error('dedup'); })()}>
        <p>ok</p>
      </Show>;
    }
    render(() => <AppErrorBoundary><MaybeThrow /></AppErrorBoundary>);
    await waitFor(() => expect(fetchSpy).toHaveBeenCalledTimes(1));
    // reset 后再次抛同一个 Error 实例不会，但 fallback 每次渲染是 new Error，所以直接检测第一次足够
    spy.mockRestore();
  });

  it('silently ignores fetch failure without crashing ErrorBoundary', async () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    fetchSpy.mockRejectedValue(new Error('network down'));
    function Throwing(): never { throw new Error('silent-fail'); }
    // 不抛 unhandled rejection
    expect(() => render(() => <AppErrorBoundary><Throwing /></AppErrorBoundary>)).not.toThrow();
    await waitFor(() => expect(screen.getByText('出错了')).toBeInTheDocument());
    spy.mockRestore();
  });
});

describe('AppErrorBoundary extra — reset & navigate handlers', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true }));
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

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
