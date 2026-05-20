import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: { checkStatus: vi.fn(), setup: vi.fn() },
}));
vi.mock('@/lib/token', () => ({
  tokenManager: { setAdminToken: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { tokenManager } from '@/lib/token';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToken = tokenManager as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('AdminSetupPage extra', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/admin/AdminSetupPage');
    return renderWithProviders(() => <Page />);
  }

  it('redirects to login when already initialized', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: true });
    await renderPage();
    await waitFor(() => expect(mockApi.checkStatus).toHaveBeenCalled());
  });

  it('shows check error UI when status fails', async () => {
    mockApi.checkStatus.mockRejectedValue(new Error('无法连接'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('连接失败')).toBeInTheDocument());
  });

  it('shows error for empty fields', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '创建管理员' })).toBeInTheDocument());
    // 输入框 required + jsdom 原生 form 校验会拦截 click submit；用 fireEvent.submit 绕过
    const form = document.querySelector('form') as HTMLFormElement;
    fireEvent.submit(form);
    await waitFor(() => expect(screen.getByText('请填写所有字段')).toBeInTheDocument());
  });

  it('shows error when password mismatch', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '创建管理员' })).toBeInTheDocument());
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'abcdefgh' } });
    fireEvent.input(inputs[2], { target: { value: 'different' } });
    fireEvent.click(screen.getByRole('button', { name: '创建管理员' }));
    await waitFor(() => expect(screen.getByText('密码不一致')).toBeInTheDocument());
  });

  it('shows error when password too short', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '创建管理员' })).toBeInTheDocument());
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: '123' } });
    fireEvent.input(inputs[2], { target: { value: '123' } });
    fireEvent.click(screen.getByRole('button', { name: '创建管理员' }));
    await waitFor(() => expect(screen.getByText(/密码至少/)).toBeInTheDocument());
  });

  it('submits setup successfully', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    mockApi.setup.mockResolvedValue({ token: 'tok' });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '创建管理员' })).toBeInTheDocument());
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'abcdefgh' } });
    fireEvent.input(inputs[2], { target: { value: 'abcdefgh' } });
    fireEvent.click(screen.getByRole('button', { name: '创建管理员' }));
    await waitFor(() => expect(mockToken.setAdminToken).toHaveBeenCalledWith('tok'));
  });

  it('shows setup api error', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    mockApi.setup.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '创建管理员' })).toBeInTheDocument());
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'abcdefgh' } });
    fireEvent.input(inputs[2], { target: { value: 'abcdefgh' } });
    fireEvent.click(screen.getByRole('button', { name: '创建管理员' }));
    await waitFor(() => expect(screen.getByText('boom')).toBeInTheDocument());
  });
});
