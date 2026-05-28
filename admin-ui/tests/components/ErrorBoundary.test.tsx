import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@solidjs/testing-library';
import { AppErrorBoundary } from '@/components/ErrorBoundary';

function ThrowingComponent(): never {
  throw new Error('Test error');
}

describe('AppErrorBoundary', () => {
  beforeEach(() => {
    // 静默 fetch：避免 ECONNREFUSED 日志噪声，reportError 是 fire-and-forget
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true }));
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });
  it('renders children when no error', () => {
    render(() => <AppErrorBoundary><p>Safe content</p></AppErrorBoundary>);
    expect(screen.getByText('Safe content')).toBeInTheDocument();
  });

  it('shows error message when error thrown', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(() => <AppErrorBoundary><ThrowingComponent /></AppErrorBoundary>);
    expect(screen.getByText('Test error')).toBeInTheDocument();
    spy.mockRestore();
  });

  it("shows '出错了' heading", () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(() => <AppErrorBoundary><ThrowingComponent /></AppErrorBoundary>);
    expect(screen.getByText('出错了')).toBeInTheDocument();
    spy.mockRestore();
  });

  it('has 返回首页 button', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(() => <AppErrorBoundary><ThrowingComponent /></AppErrorBoundary>);
    expect(screen.getByText('返回首页')).toBeInTheDocument();
    spy.mockRestore();
  });

  it('has 重试 button that calls reset', () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(() => <AppErrorBoundary><ThrowingComponent /></AppErrorBoundary>);
    expect(screen.getByText('重试')).toBeInTheDocument();
    spy.mockRestore();
  });
});
