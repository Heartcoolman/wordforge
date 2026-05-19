import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

let lastSseHandlers: any = null;

vi.mock('@/api/admin', () => ({
  adminApi: {
    updatesStatus: vi.fn(),
    updatesCheck: vi.fn(),
    updatesApply: vi.fn(),
  },
}));
vi.mock('@/api/client', async () => {
  const actual = await vi.importActual<typeof import('@/api/client')>('@/api/client');
  return {
    ...actual,
    connectSseStream: vi.fn((handlers: any) => {
      lastSseHandlers = handlers;
      return () => {};
    }),
  };
});
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const baseStatus = {
  currentVersion: '1.0.0',
  latestVersion: '1.1.0',
  hasUpdate: true,
  canApply: true,
  autoCheckEnabled: true,
  lastCheckedAt: '2026-04-15T10:00:00Z',
  releaseNotes: '修复若干 bug',
  releaseUrl: 'https://github.com/x/y/releases/v1.1.0',
};
const upgradedStatus = { ...baseStatus, currentVersion: '1.1.0' };

async function renderPage() {
  const { default: Page } = await import('@/pages/admin/UpdatesPage');
  return renderWithProviders(() => <Page />);
}

describe('UpdatesPage — status, check, apply', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lastSseHandlers = null;
  });

  it('renders status cards and release notes', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByText('当前版本')).toBeInTheDocument());
    expect(screen.getByText('1.0.0')).toBeInTheDocument();
    expect(screen.getByText('Release Notes')).toBeInTheDocument();
    expect(screen.getByText('修复若干 bug')).toBeInTheDocument();
  });

  it('shows nil values when latestVersion absent', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, latestVersion: null, lastCheckedAt: null, hasUpdate: false, releaseNotes: null });
    await renderPage();
    await waitFor(() => expect(screen.getByText('尚未检查')).toBeInTheDocument());
    expect(screen.getByText('从未检查')).toBeInTheDocument();
  });

  it('shows arch warning when hasUpdate but canApply is false', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, canApply: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/未找到匹配当前架构的产物/)).toBeInTheDocument());
  });

  it('triggers check and shows update found toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue({ ...baseStatus, hasUpdate: true });
    await renderPage();
    await waitFor(() => expect(screen.getByText('立即检查')).toBeInTheDocument());
    fireEvent.click(screen.getByText('立即检查'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('triggers check and shows up-to-date toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue({ ...baseStatus, hasUpdate: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText('立即检查')).toBeInTheDocument());
    fireEvent.click(screen.getByText('立即检查'));
    await waitFor(() => expect(mockToast.info).toHaveBeenCalled());
  });

  it('shows check failure toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockRejectedValue(new Error('net'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('立即检查')).toBeInTheDocument());
    fireEvent.click(screen.getByText('立即检查'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('opens confirm dialog and applies via 开始升级', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesApply.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/一键更新到/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/一键更新到/));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('开始升级'));
    await waitFor(() => expect(mockApi.updatesApply).toHaveBeenCalled());
  });

  it('cancels apply confirm dialog', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/一键更新到/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/一键更新到/));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
  });
});

describe('UpdatesPage — apply polling loop', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lastSseHandlers = null;
  });

  afterEach(() => { vi.useRealTimers(); });

  it('polls until currentVersion matches latestVersion then shows success toast', async () => {
    mockApi.updatesStatus.mockResolvedValueOnce(baseStatus);
    mockApi.updatesApply.mockResolvedValue(undefined);
    mockApi.updatesStatus
      .mockResolvedValueOnce(baseStatus)
      .mockResolvedValueOnce(upgradedStatus);
    mockApi.updatesStatus.mockResolvedValue(upgradedStatus);

    await renderPage();
    await waitFor(() => expect(screen.getByText(/一键更新到/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/一键更新到/));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());

    vi.useFakeTimers();
    fireEvent.click(screen.getByText('开始升级'));
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(2000);
    await vi.advanceTimersByTimeAsync(2000);
    await vi.advanceTimersByTimeAsync(1500);
    vi.useRealTimers();

    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('shows timeout error toast when polling never matches latest', async () => {
    mockApi.updatesStatus.mockResolvedValueOnce(baseStatus);
    mockApi.updatesApply.mockResolvedValue(undefined);
    mockApi.updatesStatus
      .mockRejectedValueOnce(new Error('server restarting'))
      .mockResolvedValue(baseStatus);

    const realSetTimeout = global.setTimeout;
    const stSpy = vi
      .spyOn(global, 'setTimeout')
      .mockImplementation(((fn: TimerHandler, ms?: number, ...args: unknown[]) => {
        if (ms === 2000) {
          queueMicrotask(() => {
            if (typeof fn === 'function') (fn as (...a: unknown[]) => void)(...args);
          });
          return 0 as unknown as ReturnType<typeof setTimeout>;
        }
        return realSetTimeout(fn as never, ms as never, ...(args as []));
      }) as typeof setTimeout);

    const realNow = Date.now.bind(Date);
    let nowCall = 0;
    const nowSpy = vi.spyOn(Date, 'now').mockImplementation(() => {
      nowCall += 1;
      if (nowCall === 1) return realNow();
      return realNow() + 9_999_999;
    });

    await renderPage();
    await waitFor(() => expect(screen.getByText(/一键更新到/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/一键更新到/));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('开始升级'));

    await waitFor(() => {
      const calls = mockToast.error.mock.calls;
      const hit = calls.some((c) => typeof c[0] === 'string' && c[0].includes('升级超时'));
      expect(hit).toBe(true);
    });

    nowSpy.mockRestore();
    stSpy.mockRestore();
  });
});

describe('UpdatesPage — SSE handlers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lastSseHandlers = null;
  });

  it('invokes onReleaseAvailable: info toast + refetch', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(lastSseHandlers).not.toBeNull());
    expect(typeof lastSseHandlers.onReleaseAvailable).toBe('function');
    lastSseHandlers.onReleaseAvailable();
    await waitFor(() => expect(mockToast.info).toHaveBeenCalledWith('有新版本可用，已自动刷新'));
  });

  it('invokes onUpdateProgress: sets progress state without throwing', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(lastSseHandlers).not.toBeNull());
    expect(typeof lastSseHandlers.onUpdateProgress).toBe('function');
    expect(() => lastSseHandlers.onUpdateProgress({ phase: 'download', percent: 42 })).not.toThrow();
  });
});
