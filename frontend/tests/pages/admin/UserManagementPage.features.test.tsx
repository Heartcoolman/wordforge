import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getUsers: vi.fn(),
    banUser: vi.fn(),
    unbanUser: vi.fn(),
    setUserPassword: vi.fn(),
    resetUserPassword: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const user1 = { id: 1, username: 'alice', email: 'alice@example.com', isBanned: false };
const user2 = { id: 2, username: 'bob', email: 'bob@example.com', isBanned: true };

async function renderPage() {
  const { default: Page } = await import('@/pages/admin/UserManagementPage');
  return renderWithProviders(() => <Page />);
}

describe('UserManagementPage — list, ban, reset password, pagination', () => {
  beforeEach(() => vi.clearAllMocks());

  it('lists users after loading', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1, user2], total: 2 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    expect(screen.getByText('bob')).toBeInTheDocument();
  });

  it('shows empty state when no users', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [], total: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无用户')).toBeInTheDocument());
  });

  it('shows load failure toast', async () => {
    mockApi.getUsers.mockRejectedValue(new Error('500'));
    await renderPage();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('opens ban confirm and bans user', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.banUser.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getAllByText('确认封禁').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getAllByText('确认封禁')[1]);
    await waitFor(() => expect(mockApi.banUser).toHaveBeenCalledWith(1));
  });

  it('cancels ban dialog', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getAllByText('确认封禁').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认封禁')).not.toBeInTheDocument());
  });

  it('handles ban failure', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.banUser.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    fireEvent.click(screen.getAllByText('确认封禁')[1]);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('unbans a banned user', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user2], total: 1 });
    mockApi.unbanUser.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(screen.getAllByText('确认解封').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getAllByText('确认解封')[1]);
    await waitFor(() => expect(mockApi.unbanUser).toHaveBeenCalledWith(2));
  });

  it('opens reset password modal', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    await waitFor(() => expect(screen.getByText('直接重置密码')).toBeInTheDocument());
    expect(screen.getByText('生成重置密钥')).toBeInTheDocument();
  });

  it('completes direct password reset', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.setUserPassword.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    const pwInputs = document.querySelectorAll('input[type="password"]');
    fireEvent.input(pwInputs[0], { target: { value: 'newpassword' } });
    fireEvent.input(pwInputs[1], { target: { value: 'newpassword' } });
    fireEvent.click(screen.getByText('确认重置'));
    await waitFor(() => expect(mockApi.setUserPassword).toHaveBeenCalledWith(1, 'newpassword'));
  });

  it('shows error when direct reset password empty', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    fireEvent.click(screen.getByText('确认重置'));
    await waitFor(() => expect(screen.getByText('请输入新密码')).toBeInTheDocument());
  });

  it('shows error when direct reset password mismatch', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    const pwInputs = document.querySelectorAll('input[type="password"]');
    fireEvent.input(pwInputs[0], { target: { value: 'pw1' } });
    fireEvent.input(pwInputs[1], { target: { value: 'pw2' } });
    fireEvent.click(screen.getByText('确认重置'));
    await waitFor(() => expect(screen.getByText('两次密码输入不一致')).toBeInTheDocument());
  });

  it('generates reset key', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.resetUserPassword.mockResolvedValue({ resetKey: 'abc-123' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('生成重置密钥'));
    await waitFor(() => expect(screen.getByText('abc-123')).toBeInTheDocument());
  });

  it('handles generate key failure', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.resetUserPassword.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('生成重置密钥'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('handles direct reset api failure', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.setUserPassword.mockRejectedValue(new Error('api error'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    const pwInputs = document.querySelectorAll('input[type="password"]');
    fireEvent.input(pwInputs[0], { target: { value: 'newpw' } });
    fireEvent.input(pwInputs[1], { target: { value: 'newpw' } });
    fireEvent.click(screen.getByText('确认重置'));
    await waitFor(() => expect(screen.getByText('api error')).toBeInTheDocument());
  });

  it('changes pagination page', async () => {
    const many = Array.from({ length: 50 }, (_, i) => ({ id: i, username: `u${i}`, email: `${i}@x.com`, isBanned: false }));
    mockApi.getUsers.mockResolvedValue({ data: many.slice(0, 20), total: 50 });
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('第 2 页')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('第 2 页'));
    await waitFor(() => expect(mockApi.getUsers).toHaveBeenCalledWith({ page: 2, perPage: 20 }));
  });
});

describe('UserManagementPage — reset key copy & navigation', () => {
  beforeEach(() => vi.clearAllMocks());

  it('direct-reset 返回 button goes back to choose mode', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    await waitFor(() => expect(screen.getByText('确认重置')).toBeInTheDocument());
    fireEvent.click(screen.getByText('返回'));
    await waitFor(() => expect(screen.getByText('生成重置密钥')).toBeInTheDocument());
  });

  it('copy button on reset-key result calls clipboard.writeText', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.resetUserPassword.mockResolvedValue({ resetKey: 'XYZ-789' });
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    await renderPage();
    await waitFor(() => expect(screen.getByText('重置密码')).toBeInTheDocument());
    fireEvent.click(screen.getByText('重置密码'));
    fireEvent.click(screen.getByText('生成重置密钥'));
    await waitFor(() => expect(screen.getByText('XYZ-789')).toBeInTheDocument());
    fireEvent.click(screen.getByText('复制'));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('XYZ-789'));
    expect(mockToast.success).toHaveBeenCalledWith('已复制到剪贴板');
  });

  it('copy button falls back to error toast on clipboard failure', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.resetUserPassword.mockResolvedValue({ resetKey: 'KEY-FAIL' });
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error('no perm')) },
    });
    await renderPage();
    fireEvent.click(await screen.findByText('重置密码'));
    fireEvent.click(screen.getByText('生成重置密钥'));
    await waitFor(() => expect(screen.getByText('KEY-FAIL')).toBeInTheDocument());
    fireEvent.click(screen.getByText('复制'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('复制失败', '请手动选择并复制'));
  });
});
