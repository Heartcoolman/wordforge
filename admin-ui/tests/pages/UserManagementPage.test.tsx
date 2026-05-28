import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// m024:UserManagementPage 现在拉 getStats + getEngagement(顶部 KPI 行),getUsers
// 走 ?includeStats=true。Drawer 拉 userProfile / userSessions。批量操作端点
// usersBulkBan / usersBulkUnban / usersBulkRole。新增 createUser/patchUserRole
// /userDevices/userAuditLog 7 个 method。mock 默认 resolve 空值,具体用例可 override。
vi.mock('@/api/admin', () => ({
  adminApi: {
    getUsers: vi.fn(),
    banUser: vi.fn(),
    unbanUser: vi.fn(),
    setUserPassword: vi.fn(),
    resetUserPassword: vi.fn(),
    getStats: vi.fn(() => Promise.resolve({ users: 100, words: 0, records: 0, trend: { users: { value: 5, label: '较昨日' } } })),
    getEngagement: vi.fn(() => Promise.resolve({ totalUsers: 100, activeToday: 12, retentionRate: 0.12 })),
    userProfile: vi.fn(() => Promise.resolve({ userId: 'x', totalRecords: 0, correctRecords: 0, accuracy: null, sessionCount: 0, wordbookDistribution: [] })),
    userSessions: vi.fn(() => Promise.resolve({ sessions: [] })),
    usersBulkBan: vi.fn(),
    usersBulkUnban: vi.fn(),
    createUser: vi.fn(),
    patchUserRole: vi.fn(),
    usersBulkRole: vi.fn(),
    userDevices: vi.fn(() => Promise.resolve({ devices: [] })),
    userAuditLog: vi.fn(() => Promise.resolve({ entries: [] })),
    // m025:Drawer 资料/答题/设备/日志 4 tab 深化数据源(默认空)
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

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

// m024:AdminUser 扩展字段 role/status/lastLoginAt/stats
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
  perPage: 20,
  totalPages: 1,
};

describe('UserManagementPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: UserManagementPage } = await import('@/pages/UserManagementPage');
    return renderWithProviders(() => <UserManagementPage />);
  }

  it('renders 9-column table after loading', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // m024:9 列 redesign,'用户' 替代 '用户名',加 '角色' / '注册' / '最近活跃' / '答题数' / '正确率' / '最近 20 题'
    await waitFor(() => expect(screen.getByText('用户')).toBeInTheDocument());
  });

  it('shows loading spinner initially', async () => {
    mockAdminApi.getUsers.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows user rows after loading', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    expect(screen.getByText('bob')).toBeInTheDocument();
  });

  it('shows new column headers', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByText('用户')).toBeInTheDocument());
    expect(screen.getByText('角色')).toBeInTheDocument();
    expect(screen.getByText('注册')).toBeInTheDocument();
    expect(screen.getByText('最近活跃')).toBeInTheDocument();
    expect(screen.getByText('答题数')).toBeInTheDocument();
    expect(screen.getByText('正确率')).toBeInTheDocument();
    expect(screen.getByText('最近 20 题')).toBeInTheDocument();
  });

  it('shows "封禁" option in row menu for active users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // m026:inline 操作改为三点菜单,先打开行菜单再断言
    await waitFor(() => expect(screen.getByLabelText('alice 更多操作')).toBeInTheDocument());
    const { fireEvent } = await import('@solidjs/testing-library');
    fireEvent.click(screen.getByLabelText('alice 更多操作'));
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
  });

  it('shows "解封" option in row menu for banned users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByLabelText('bob 更多操作')).toBeInTheDocument());
    const { fireEvent } = await import('@solidjs/testing-library');
    fireEvent.click(screen.getByLabelText('bob 更多操作'));
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
  });

  it('shows status badges for users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // m024:active 用户显示"活跃"(也存在于 chip filter,用 getAllByText),banned 用户显示"已封禁"
    await waitFor(() => expect(screen.getAllByText('活跃').length).toBeGreaterThan(0));
    expect(screen.getByText('已封禁')).toBeInTheDocument();
  });

  it('shows role chips for users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    // m024:role chip 显示中文 label '普通'(mockUsers 全是 role:user)
    await waitFor(() => {
      const normalChips = screen.getAllByText('普通');
      expect(normalChips.length).toBeGreaterThan(0);
    });
  });

  it('shows chip filters', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByText('全部')).toBeInTheDocument());
    expect(screen.getByText('7 天未登录')).toBeInTheDocument();
    expect(screen.getByText('管理员')).toBeInTheDocument();
  });

  it('shows "添加用户" button in header', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => expect(screen.getByText('+ 添加用户')).toBeInTheDocument());
  });
});
