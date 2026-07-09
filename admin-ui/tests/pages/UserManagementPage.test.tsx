import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 重设计后的 UserManagementPage 调用:
//   - userFacets()        chip 分面计数
//   - getUsers(query)     列表(includeStats=true)
//   - banUser / unbanUser 行内封禁/解封
//   - createUser          添加用户 modal
//   - patchUserRole       Drawer 内角色变更
//   - resetUserPassword   重置密码
//   - usersBulkBan/Unban/ResetPassword/Delete  批量操作
//   - Drawer 数据源:userProfile / userExtras / userSessions / userDevices /
//     userAuditLog / userActivityLog
// 全部 mock 给安全默认值,避免 unhandled rejection。
vi.mock('@/api/admin', () => ({
  adminApi: {
    userFacets: vi.fn(() => Promise.resolve({ total: 2, active: 1, inactive7d: 0, banned: 1, admins: 0 })),
    getUsers: vi.fn(() => Promise.resolve({ data: [], total: 0, page: 1, perPage: 12, totalPages: 1 })),
    createUser: vi.fn(() => Promise.resolve({ id: 'new' })),
    banUser: vi.fn(() => Promise.resolve({ banned: true, userId: '1' })),
    unbanUser: vi.fn(() => Promise.resolve({ banned: false, userId: '2' })),
    resetUserPassword: vi.fn(() => Promise.resolve({ resetKey: 'k', expiresInHours: 24 })),
    patchUserRole: vi.fn(() => Promise.resolve({})),
    usersBulkBan: vi.fn(() => Promise.resolve({ succeeded: 1, failed: 0 })),
    usersBulkUnban: vi.fn(() => Promise.resolve({ succeeded: 1, failed: 0 })),
    usersBulkResetPassword: vi.fn(() => Promise.resolve({ succeeded: 1, failed: 0 })),
    usersBulkDelete: vi.fn(() => Promise.resolve({ succeeded: 1, failed: 0 })),
    usersBulkRole: vi.fn(() => Promise.resolve({ succeeded: 1, failed: 0 })),
    userProfile: vi.fn(() => Promise.resolve({ userId: 'x', totalRecords: 0, correctRecords: 0, accuracy: null, sessionCount: 0, avgResponseTimeMs: null, wordbookDistribution: [] })),
    userExtras: vi.fn(() => Promise.resolve({
      preferences: null, elo: null, habit: null, streakDays: 0,
      latestStrategy: null, latestDevice: null,
      metrics7d: { records: 0, correct: 0, accuracy: null, fatigueAlertCount: 0 },
    })),
    userSessions: vi.fn(() => Promise.resolve({ sessions: [] })),
    userDevices: vi.fn(() => Promise.resolve({ devices: [] })),
    userAuditLog: vi.fn(() => Promise.resolve({ entries: [] })),
    userActivityLog: vi.fn(() => Promise.resolve({ entries: [] })),
    banUserDevice: vi.fn(() => Promise.resolve({})),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const baseUser = {
  failedLoginCount: 0,
  lockedUntil: null,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
  role: 'user' as const,
  status: 'active' as const,
  lastLoginAt: null,
};
const mockUsers = {
  data: [
    { ...baseUser, id: '1', username: 'alice', email: 'alice@test.com', isBanned: false, stats: { recordCount: 100, correctCount: 80, last20Outcomes: [1, 1, 0, 1, 1] } },
    { ...baseUser, id: '2', username: 'bob', email: 'bob@test.com', isBanned: true, status: 'suspended' as const },
  ],
  total: 2,
  page: 1,
  perPage: 12,
  totalPages: 1,
};

describe('UserManagementPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // clearAllMocks 也清掉了 mock 实现,这里重置回安全默认值
    mockAdminApi.userFacets.mockResolvedValue({ total: 2, active: 1, inactive7d: 0, banned: 1, admins: 0 });
    mockAdminApi.getUsers.mockResolvedValue({ data: [], total: 0, page: 1, perPage: 12, totalPages: 1 });
    mockAdminApi.banUser.mockResolvedValue({ banned: true, userId: '1' });
    mockAdminApi.unbanUser.mockResolvedValue({ banned: false, userId: '2' });
    mockAdminApi.createUser.mockResolvedValue({ id: 'new' });
    mockAdminApi.resetUserPassword.mockResolvedValue({ resetKey: 'k', expiresInHours: 24 });
    mockAdminApi.patchUserRole.mockResolvedValue({});
    mockAdminApi.userProfile.mockResolvedValue({ userId: 'x', totalRecords: 0, correctRecords: 0, accuracy: null, sessionCount: 0, avgResponseTimeMs: null, wordbookDistribution: [] });
    mockAdminApi.userExtras.mockResolvedValue({ preferences: null, elo: null, habit: null, streakDays: 0, latestStrategy: null, latestDevice: null, metrics7d: { records: 0, correct: 0, accuracy: null, fatigueAlertCount: 0 } });
    mockAdminApi.userSessions.mockResolvedValue({ sessions: [] });
    mockAdminApi.userDevices.mockResolvedValue({ devices: [] });
    mockAdminApi.userAuditLog.mockResolvedValue({ entries: [] });
    mockAdminApi.userActivityLog.mockResolvedValue({ entries: [] });
  });

  async function renderPage() {
    const { default: UserManagementPage } = await import('@/pages/UserManagementPage');
    return renderWithProviders(() => <UserManagementPage />);
  }

  it('renders table header after loading', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // 列头 '用户' 替代旧 '用户名'
    await waitFor(() => expect(screen.getByRole('columnheader', { name: '用户' })).toBeInTheDocument());
  });

  it('shows skeleton rows while loading', async () => {
    // getUsers 永不 resolve,断言列表区处于加载态(渲染 skel 占位行而非真实用户)
    mockAdminApi.getUsers.mockReturnValue(new Promise(() => {}));
    const { container } = await renderPage();
    await waitFor(() => expect(container.querySelector('.skel')).toBeTruthy());
    expect(screen.queryByText('alice')).not.toBeInTheDocument();
  });

  it('shows user rows after loading', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    expect(screen.getByText('bob')).toBeInTheDocument();
    expect(screen.getByText('alice@test.com')).toBeInTheDocument();
  });

  it('shows new column headers', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('columnheader', { name: '用户' })).toBeInTheDocument());
    expect(screen.getByRole('columnheader', { name: '角色' })).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: '状态' })).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: '答题数' })).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: '正确率' })).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: '最后登录' })).toBeInTheDocument();
    expect(screen.getByRole('columnheader', { name: '注册时间' })).toBeInTheDocument();
  });

  it('shows "封禁" inline action for active users and calls banUser', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // alice 未封禁 => 行内显示 '封禁' 按钮
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    const banBtn = screen.getByRole('button', { name: '封禁' });
    expect(banBtn).toBeInTheDocument();
    fireEvent.click(banBtn);
    await waitFor(() => expect(mockAdminApi.banUser).toHaveBeenCalledWith('1'));
  });

  it('shows "解封" inline action for banned users and calls unbanUser', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // bob 已封禁 => 行内显示 '解封' 按钮
    await waitFor(() => expect(screen.getByText('bob')).toBeInTheDocument());
    const unbanBtn = screen.getByRole('button', { name: '解封' });
    expect(unbanBtn).toBeInTheDocument();
    fireEvent.click(unbanBtn);
    await waitFor(() => expect(mockAdminApi.unbanUser).toHaveBeenCalledWith('2'));
  });

  it('shows status badges for users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // active 用户显示 '正常',banned 用户显示 '已封禁'(也出现在 chip,用 getAllByText)
    await waitFor(() => expect(screen.getByText('正常')).toBeInTheDocument());
    expect(screen.getAllByText('已封禁').length).toBeGreaterThan(0);
  });

  it('shows role badges for users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // 两个 mock 用户均为 role:user => badge 文案 '普通用户'
    await waitFor(() => {
      const chips = screen.getAllByText('普通用户');
      expect(chips.length).toBeGreaterThan(0);
    });
  });

  it('shows chip filters', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByText('全部')).toBeInTheDocument());
    expect(screen.getByText('7 天未登录')).toBeInTheDocument();
    // '管理员' 同时出现在 chip 与 role 下拉,用 getAllByText
    expect(screen.getAllByText('管理员').length).toBeGreaterThan(0);
  });

  it('shows "添加用户" button in header and opens create modal', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    const addBtn = await screen.findByRole('button', { name: '添加用户' });
    expect(addBtn).toBeInTheDocument();
    fireEvent.click(addBtn);
    // modal 打开 => 出现表单字段
    await waitFor(() => expect(screen.getByText('邮箱')).toBeInTheDocument());
    expect(screen.getByText('用户名')).toBeInTheDocument();
    expect(screen.getByText('初始密码')).toBeInTheDocument();
  });
});
