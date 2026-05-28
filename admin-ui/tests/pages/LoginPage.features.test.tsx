import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    login: vi.fn(),
    getHealth: vi.fn(),
    checkStatus: vi.fn(),
  },
}));
vi.mock('@/api/health', () => ({
  healthApi: {
    getStatus: vi.fn(() => Promise.resolve(null)),
  },
}));
vi.mock('@/lib/token', () => ({
  tokenManager: {
    getAdminToken: vi.fn(() => null),
    setAdminToken: vi.fn(),
    clearAdminToken: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';

const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToken = tokenManager as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('LoginPage extra branches', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockToken.getAdminToken.mockReturnValue(null);
    mockApi.checkStatus.mockResolvedValue({ initialized: true });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/LoginPage');
    return renderWithProviders(() => <Page />);
  }

  it('redirects to /admin when existing token verifies', async () => {
    mockToken.getAdminToken.mockReturnValue('tok');
    mockApi.getHealth.mockResolvedValue({ status: 'healthy' });
    await renderPage();
    await waitFor(() => expect(mockApi.getHealth).toHaveBeenCalled());
  });

  it('clears token when getHealth fails', async () => {
    mockToken.getAdminToken.mockReturnValue('tok');
    mockApi.getHealth.mockRejectedValue(new Error('401'));
    await renderPage();
    await waitFor(() => expect(mockToken.clearAdminToken).toHaveBeenCalled());
  });

  it('redirects to setup when not initialized', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    await renderPage();
    await waitFor(() => expect(mockApi.checkStatus).toHaveBeenCalled());
  });

  it('shows error when submit empty form', async () => {
    await renderPage();
    // Input required + jsdom 原生 form 校验会拦截 click submit；用 fireEvent.submit 绕过
    const form = document.querySelector('form') as HTMLFormElement;
    fireEvent.submit(form);
    // 拆分后：邮箱空时只触发 emailError（字段级），文案改为"请填写邮箱"
    await waitFor(() => expect(screen.getByText('请填写邮箱')).toBeInTheDocument());
  });

  it('submits successfully', async () => {
    mockApi.login.mockResolvedValue({ token: 'new-tok' });
    await renderPage();
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'pwd123' } });
    fireEvent.click(screen.getByRole('button', { name: /进入管理后台/ }));
    await waitFor(() => expect(mockToken.setAdminToken).toHaveBeenCalledWith('new-tok'));
  });

  it('on failure increments fail count and locks', async () => {
    mockApi.login.mockRejectedValue(new Error('密码错误'));
    await renderPage();
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'wrong' } });
    fireEvent.click(screen.getByRole('button', { name: /进入管理后台/ }));
    await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument());
  });

  it('shows count-aware message after 3 failures', async () => {
    // 第 i 次（1-based）失败后 lockSeconds = i，需推进 >i 秒让 setInterval 归零
    // 用 vi.useFakeTimers 同时 mock Date.now，保证 lockUntil 比较正确
    vi.useFakeTimers();
    try {
      mockApi.login.mockRejectedValue(new Error('错'));
      await renderPage();
      const inputs = document.querySelectorAll('input');
      fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
      fireEvent.input(inputs[1], { target: { value: 'x' } });

      for (let i = 0; i < 3; i++) {
        // 按钮可能是"进入管理后台"（未锁）或前一轮已清零后的同名（不在 lock 窗口内）
        fireEvent.click(screen.getByRole('button', { name: /^(进入管理后台|锁定中)/ }));
        // 排空微任务：让 handleSubmit 的 catch 分支（Promise rejection）完成
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
        // 推进 setInterval：(i+1)*1000ms 解锁 + 额外 1500ms 确保至少一次 tick
        await vi.advanceTimersByTimeAsync((i + 1) * 1000 + 1500);
      }

      await waitFor(() => {
        const alert = screen.queryByRole('alert');
        expect(alert?.textContent).toMatch(/已失败/);
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it('rejects submit when within lock window', async () => {
    mockApi.login.mockRejectedValue(new Error('错'));
    await renderPage();
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'x' } });
    // 第一次失败后按钮文案变为"锁定中 Xs"，且按钮 disabled；
    // 用 form submit 模拟键盘 Enter 绕过 disabled，进入"请等待"分支
    const form = document.querySelector('form') as HTMLFormElement;
    fireEvent.submit(form);
    await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument());
    // 第二次立即 submit → 在锁定窗口内
    fireEvent.submit(form);
    await waitFor(() => {
      // 锁定提示 formError 渲染在顶部 alert 容器中；可能存在多个 role="alert"（字段级），
      // 用文案匹配；"请等待 X 秒后重试"
      expect(screen.getByText(/请等待/)).toBeInTheDocument();
    });
  });
});
