import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    updatesStatus: vi.fn(),
    updatesCheck: vi.fn(),
    updatesApply: vi.fn(),
  },
}));

// 保留真实 ApiError class（instanceof 判断需要），只 mock connectSseStream
vi.mock('@/api/client', async () => {
  const actual = await vi.importActual<typeof import('@/api/client')>('@/api/client');
  return {
    ...actual,
    connectSseStream: vi.fn(() => () => {}),
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

const upgradedStatus = {
  ...baseStatus,
  currentVersion: '1.1.0',
};

describe('UpdatesPage extra2 (apply polling loop)', () => {
  beforeEach(() => vi.clearAllMocks());

  afterEach(() => {
    vi.useRealTimers();
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/admin/UpdatesPage');
    return renderWithProviders(() => <Page />);
  }

  // 模拟 apply 成功后 polling 看到版本切换 → 触发 success toast + setTimeout(reload)
  it('polls until currentVersion matches latestVersion then shows success toast', async () => {
    mockApi.updatesStatus.mockResolvedValueOnce(baseStatus); // 初次加载
    mockApi.updatesApply.mockResolvedValue(undefined);
    // polling 第一次还在旧版本 → 第二次切到新版本（success 分支）
    mockApi.updatesStatus
      .mockResolvedValueOnce(baseStatus) // poll #1（不匹配）
      .mockResolvedValueOnce(upgradedStatus); // poll #2（匹配）
    // refetch after success
    mockApi.updatesStatus.mockResolvedValue(upgradedStatus);

    await renderPage();
    await waitFor(() => expect(screen.getByText(/一键更新到/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/一键更新到/));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());

    // 切换到 fake timers，避免真的等 2 秒
    vi.useFakeTimers();
    fireEvent.click(screen.getByText('开始升级'));

    // 让 confirmApply 跑起来：先消化 updatesApply 的 microtask
    await vi.advanceTimersByTimeAsync(0);
    // 第一轮 polling delay
    await vi.advanceTimersByTimeAsync(2000);
    // 第二轮 polling delay → 命中 success
    await vi.advanceTimersByTimeAsync(2000);
    // success 分支的 setTimeout(reload, 1500)
    await vi.advanceTimersByTimeAsync(1500);

    vi.useRealTimers();

    await waitFor(() => {
      expect(mockToast.success).toHaveBeenCalled();
    });
  });

  // 模拟 apply 成功后 polling 始终拿到旧版本（或抛错）→ deadline 超时 → error toast
  // 思路：把 polling 用的 sleep setTimeout 替换为立即 resolve，
  // 并 stub Date.now 让 while-loop 在第二轮立即过期。
  it('shows timeout error toast when polling never matches latest', async () => {
    mockApi.updatesStatus.mockResolvedValueOnce(baseStatus); // 初次加载
    mockApi.updatesApply.mockResolvedValue(undefined);
    // polling 第一次抛错触发 catch（line 97-99），后续仍是旧版本
    mockApi.updatesStatus
      .mockRejectedValueOnce(new Error('server restarting'))
      .mockResolvedValue(baseStatus);

    // 用 narrow patch：只把 2000ms 的 setTimeout（confirmApply 的 sleep）改成立即 resolve；
    // 其它调用走原实现，避免影响 waitFor / SolidJS
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

    // Date.now：第 1 次返回正常时间（deadline 计算），之后跳到 deadline 之后
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
