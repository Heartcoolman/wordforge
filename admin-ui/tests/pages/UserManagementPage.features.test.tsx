import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// m024:UserManagementPage 大改 —— 顶部 KPI / chip filter / 9 列 / 添加用户 / 角色变更。
// 新依赖:getStats / getEngagement / userProfile / userSessions / usersBulkBan
// / usersBulkUnban / createUser / patchUserRole / usersBulkRole / userDevices /
// userAuditLog。默认 resolve 空值,具体用例可 override。
vi.mock('@/api/admin', () => ({
  adminApi: {
    getUsers: vi.fn(),
    banUser: vi.fn(),
    unbanUser: vi.fn(),
    setUserPassword: vi.fn(),
    resetUserPassword: vi.fn(),
    getStats: vi.fn(() => Promise.resolve({ users: 100, words: 0, records: 0, trend: { users: { value: 5, label: '较昨日' } } })),
    getEngagement: vi.fn(() => Promise.resolve({ totalUsers: 100, activeToday: 12, retentionRate: 0.12 })),
    // m027:header "本周新增" 取近 7 天 daily-active-users 的 registered 求和
    getDailyActiveUsers: vi.fn(() => Promise.resolve([{ date: '2026-05-30', active: 10, registered: 3 }])),
    // m027:筛选 chip 逐项计数 GET /users/facets
    userFacets: vi.fn(() => Promise.resolve({ total: 100, active: 80, inactive7d: 12, banned: 5, admins: 3 })),
    userProfile: vi.fn(() => Promise.resolve({ userId: 'x', totalRecords: 0, correctRecords: 0, accuracy: null, sessionCount: 0, wordbookDistribution: [] })),
    userSessions: vi.fn(() => Promise.resolve({ sessions: [] })),
    usersBulkBan: vi.fn(),
    usersBulkUnban: vi.fn(),
    createUser: vi.fn(),
    patchUserRole: vi.fn(),
    usersBulkRole: vi.fn(),
    userDevices: vi.fn(() => Promise.resolve({ devices: [] })),
    userAuditLog: vi.fn(() => Promise.resolve({ entries: [] })),
    // m025:Drawer 4 tab 深化数据源(默认空)
    userExtras: vi.fn(() => Promise.resolve({
      preferences: null, elo: null, habit: null, streakDays: 0,
      latestStrategy: null, latestDevice: null,
      metrics7d: { records: 0, correct: 0, accuracy: null, fatigueAlertCount: 0 },
    })),
    userActivityLog: vi.fn(() => Promise.resolve({ entries: [] })),
    // m026:批量重置/删除/设备封禁
    usersBulkResetPassword: vi.fn(),
    usersBulkDelete: vi.fn(),
    banUserDevice: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

// m024:AdminUser 字段扩展;id 仍为 string
const baseUser = {
  failedLoginCount: 0,
  lockedUntil: null,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
  role: 'user' as const,
  status: 'active' as const,
  lastLoginAt: null,
};
const user1 = { ...baseUser, id: '1', username: 'alice', email: 'alice@example.com', isBanned: false };
const user2 = { ...baseUser, id: '2', username: 'bob', email: 'bob@example.com', isBanned: true, status: 'suspended' as const };

async function renderPage() {
  const { default: Page } = await import('@/pages/UserManagementPage');
  return renderWithProviders(() => <Page />);
}

/** m026:行级操作由 inline 按钮迁到三点菜单 → 触发操作前先打开该行菜单 */
function openRowMenu(username: string) {
  fireEvent.click(screen.getByLabelText(`${username} 更多操作`));
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
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('封禁'));
    await waitFor(() => expect(screen.getAllByText('确认封禁').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getAllByText('确认封禁')[1]);
    await waitFor(() => expect(mockApi.banUser).toHaveBeenCalledWith('1'));
  });

  it('cancels ban dialog', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('封禁'));
    await waitFor(() => expect(screen.getAllByText('确认封禁').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认封禁')).not.toBeInTheDocument());
  });

  it('handles ban failure', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.banUser.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('封禁'));
    fireEvent.click(screen.getAllByText('确认封禁')[1]);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('unbans a banned user', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user2], total: 1 });
    mockApi.unbanUser.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('bob 更多操作')).toBeInTheDocument());
    openRowMenu('bob');
    fireEvent.click(await screen.findByText('解封'));
    await waitFor(() => expect(screen.getAllByText('确认解封').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getAllByText('确认解封')[1]);
    await waitFor(() => expect(mockApi.unbanUser).toHaveBeenCalledWith('2'));
  });

  it('opens reset password modal', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
    await waitFor(() => expect(screen.getByText('直接重置密码')).toBeInTheDocument());
    expect(screen.getByText('生成重置密钥')).toBeInTheDocument();
  });

  it('completes direct password reset', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.setUserPassword.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    const pwInputs = document.querySelectorAll('input[type="password"]');
    fireEvent.input(pwInputs[0], { target: { value: 'newpassword' } });
    fireEvent.input(pwInputs[1], { target: { value: 'newpassword' } });
    fireEvent.click(screen.getByText('确认重置'));
    await waitFor(() => expect(mockApi.setUserPassword).toHaveBeenCalledWith('1', 'newpassword'));
  });

  it('shows error when direct reset password empty', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    fireEvent.click(screen.getByText('确认重置'));
    await waitFor(() => expect(screen.getByText('请输入新密码')).toBeInTheDocument());
  });

  it('shows error when direct reset password mismatch', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
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
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
    fireEvent.click(screen.getByText('生成重置密钥'));
    await waitFor(() => expect(screen.getByText('abc-123')).toBeInTheDocument());
  });

  it('handles generate key failure', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.resetUserPassword.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
    fireEvent.click(screen.getByText('生成重置密钥'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('handles direct reset api failure', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.setUserPassword.mockRejectedValue(new Error('api error'));
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
    fireEvent.click(screen.getByText('直接重置密码'));
    const pwInputs = document.querySelectorAll('input[type="password"]');
    fireEvent.input(pwInputs[0], { target: { value: 'newpw' } });
    fireEvent.input(pwInputs[1], { target: { value: 'newpw' } });
    fireEvent.click(screen.getByText('确认重置'));
    await waitFor(() => expect(screen.getByText('api error')).toBeInTheDocument());
  });

  it('changes pagination page', async () => {
    const many = Array.from({ length: 50 }, (_, i) => ({ ...baseUser, id: String(i), username: `u${i}`, email: `${i}@x.com`, isBanned: false }));
    mockApi.getUsers.mockResolvedValue({ data: many.slice(0, 20), total: 50 });
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('第 2 页')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('第 2 页'));
    // m024:翻页时 query 形态变更,含 includeStats / search 等
    await waitFor(() =>
      expect(mockApi.getUsers).toHaveBeenLastCalledWith(
        expect.objectContaining({ page: 2, perPage: 20, includeStats: true }),
      ),
    );
  });
});

describe('UserManagementPage — m024 新功能', () => {
  beforeEach(() => vi.clearAllMocks());

  it('switches filter chip to "管理员" and re-loads with role=admin', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('管理员')).toBeInTheDocument());
    fireEvent.click(screen.getByText('管理员'));
    await waitFor(() =>
      expect(mockApi.getUsers).toHaveBeenLastCalledWith(
        expect.objectContaining({ role: 'admin' }),
      ),
    );
  });

  it('opens create-user modal and submits', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    mockApi.createUser.mockResolvedValue({ ...baseUser, id: '99', username: 'new', email: 'new@x.com', isBanned: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText('+ 添加用户')).toBeInTheDocument());
    fireEvent.click(screen.getByText('+ 添加用户'));
    await waitFor(() => expect(screen.getByText('添加用户')).toBeInTheDocument());
    const inputs = document.querySelectorAll('input');
    // 0: 顶部搜索(可能不存在,如果 Modal 替换 focus 也 OK),所以按 type 找
    const email = document.querySelector('input[type="email"]') as HTMLInputElement;
    const pw = document.querySelectorAll('input[type="password"]');
    // 用户名是普通 text input,在 modal 内取倒数(modal 后新增的 input)
    const textInputs = Array.from(document.querySelectorAll('input[type="text"], input:not([type])')) as HTMLInputElement[];
    const usernameInput = textInputs[textInputs.length - 1]; // modal 内最后一个 text input
    expect(inputs.length).toBeGreaterThan(1);
    fireEvent.input(email, { target: { value: 'new@x.com' } });
    fireEvent.input(usernameInput, { target: { value: 'newuser' } });
    fireEvent.input(pw[0], { target: { value: 'NewPass123' } });
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() => expect(mockApi.createUser).toHaveBeenCalled());
  });
});

describe('UserManagementPage — reset key copy & navigation', () => {
  beforeEach(() => vi.clearAllMocks());

  it('direct-reset 返回 button goes back to choose mode', async () => {
    mockApi.getUsers.mockResolvedValue({ data: [user1], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
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
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
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
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    openRowMenu('alice');
    fireEvent.click(await screen.findByText('重置密码'));
    fireEvent.click(screen.getByText('生成重置密钥'));
    await waitFor(() => expect(screen.getByText('KEY-FAIL')).toBeInTheDocument());
    fireEvent.click(screen.getByText('复制'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('复制失败', '请手动选择并复制'));
  });
});
