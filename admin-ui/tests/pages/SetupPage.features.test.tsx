import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: { checkStatus: vi.fn(), setup: vi.fn(), setupEnvCheck: vi.fn() },
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

/** 5 项环境自检全 OK（对齐后端固定顺序） */
const OK_CHECKS = {
  checks: [
    { key: 'db_schema', label: '数据库 schema', ok: true, detail: 'migrations up-to-date' },
    { key: 'static_writable', label: 'static 可写', ok: true, detail: 'ok' },
    { key: 'listening_port', label: '监听端口', ok: true, detail: ':3000' },
    { key: 'signing_key', label: 'JWT signing key', ok: true, detail: 'present' },
    { key: 'admin_table', label: 'admin 表', ok: true, detail: 'empty' },
  ],
};

describe('SetupPage extra', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/SetupPage');
    return renderWithProviders(() => <Page />);
  }

  /**
   * PR redesign: SetupPage 不再是按钮门控的多步 wizard。
   * onMount 先 checkStatus（未初始化则继续）再 setupEnvCheck 拉环境自检，
   * step !== 3 时表单卡（含三个输入 + "创建管理员并登录"按钮）直接渲染。
   * 此 helper 等表单就绪（环境自检完成后表单始终可见）。
   */
  async function waitForForm() {
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /创建管理员并登录/ })).toBeInTheDocument(),
    );
  }

  it('redirects to login when already initialized', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: true });
    await renderPage();
    await waitFor(() => expect(mockApi.checkStatus).toHaveBeenCalled());
    // 已初始化时不应再拉环境自检
    expect(mockApi.setupEnvCheck).not.toHaveBeenCalled();
  });

  it('shows check error UI when status fails', async () => {
    mockApi.checkStatus.mockRejectedValue(new Error('无法连接'));
    await renderPage();
    // 自检失败时展示错误原文 + "重试"按钮
    await waitFor(() => expect(screen.getByText('无法连接')).toBeInTheDocument());
    expect(screen.getByRole('button', { name: '重试' })).toBeInTheDocument();
  });

  it('disables submit when fields are empty', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    mockApi.setupEnvCheck.mockResolvedValue(OK_CHECKS);
    await renderPage();
    await waitForForm();
    // 空表单：canSubmit() 为 false → 提交按钮禁用，不会触发任何 setup 调用
    const btn = screen.getByRole('button', { name: /创建管理员并登录/ });
    expect(btn).toBeDisabled();
    const form = document.querySelector('form') as HTMLFormElement;
    fireEvent.submit(form);
    expect(mockApi.setup).not.toHaveBeenCalled();
  });

  it('shows error when password mismatch', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    mockApi.setupEnvCheck.mockResolvedValue(OK_CHECKS);
    await renderPage();
    await waitForForm();
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'abcdefgh' } });
    fireEvent.input(inputs[2], { target: { value: 'different' } });
    // 确认密码内联错误在 blur 后展示
    fireEvent.blur(inputs[2]);
    await waitFor(() => expect(screen.getByText('两次输入不一致')).toBeInTheDocument());
    // 不一致时提交按钮禁用，setup 不应被调用
    expect(screen.getByRole('button', { name: /创建管理员并登录/ })).toBeDisabled();
  });

  it('shows error when password too short', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    mockApi.setupEnvCheck.mockResolvedValue(OK_CHECKS);
    await renderPage();
    await waitForForm();
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: '123' } });
    fireEvent.input(inputs[2], { target: { value: '123' } });
    // 密码强度计提示太短（至少 8 位），且提交禁用
    await waitFor(() => expect(screen.getByText(/太短 · 至少 8 位/)).toBeInTheDocument());
    expect(screen.getByRole('button', { name: /创建管理员并登录/ })).toBeDisabled();
  });

  it('submits setup successfully', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    mockApi.setupEnvCheck.mockResolvedValue(OK_CHECKS);
    mockApi.setup.mockResolvedValue({ token: 'tok' });
    await renderPage();
    await waitForForm();
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'abcdefgh' } });
    fireEvent.input(inputs[2], { target: { value: 'abcdefgh' } });
    fireEvent.click(screen.getByRole('button', { name: /创建管理员并登录/ }));
    await waitFor(() =>
      expect(mockApi.setup).toHaveBeenCalledWith({ email: 'a@b.c', password: 'abcdefgh' }),
    );
    await waitFor(() => expect(mockToken.setAdminToken).toHaveBeenCalledWith('tok'));
    // 成功态：step 3 成功文案
    await waitFor(() => expect(screen.getByText('管理员账户已创建')).toBeInTheDocument());
  });

  it('shows setup api error', async () => {
    mockApi.checkStatus.mockResolvedValue({ initialized: false });
    mockApi.setupEnvCheck.mockResolvedValue(OK_CHECKS);
    mockApi.setup.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitForForm();
    const inputs = document.querySelectorAll('input');
    fireEvent.input(inputs[0], { target: { value: 'a@b.c' } });
    fireEvent.input(inputs[1], { target: { value: 'abcdefgh' } });
    fireEvent.input(inputs[2], { target: { value: 'abcdefgh' } });
    fireEvent.click(screen.getByRole('button', { name: /创建管理员并登录/ }));
    await waitFor(() => expect(screen.getByText('boom')).toBeInTheDocument());
    expect(mockToken.setAdminToken).not.toHaveBeenCalled();
  });
});
