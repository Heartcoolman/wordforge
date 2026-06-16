import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 用户管理页重设计后:PageHead + facet chip + 9 列表格 + 行内「详情/封禁」按钮
// + 批量栏(Confirm) + 添加用户 Modal + 详情 Drawer(重置密码→密钥 Modal)。
// 行级封禁/解封改为直接调用(无确认弹窗);重置密码迁移到 Drawer footer,
// 返回一次性密钥 + 复制密钥。下方默认 resolve 安全值,具体用例可 override。
vi.mock('@/api/admin', () => ({
  adminApi: {
    getUsers: vi.fn(),
    userFacets: vi.fn(() => Promise.resolve({ total: 0, active: 0, inactive7d: 0, banned: 0, admins: 0 })),
    banUser: vi.fn(() => Promise.resolve({ banned: true, userId: '1' })),
    unbanUser: vi.fn(() => Promise.resolve({ banned: false, userId: '2' })),
    resetUserPassword: vi.fn(() => Promise.resolve({ resetKey: 'KEY', expiresInHours: 24 })),
    createUser: vi.fn(),
    patchUserRole: vi.fn(() => Promise.resolve({})),
    usersBulkBan: vi.fn(() => Promise.resolve({ total: 1, succeeded: 1, failed: 0, results: [] })),
    usersBulkUnban: vi.fn(() => Promise.resolve({ total: 1, succeeded: 1, failed: 0, results: [] })),
    usersBulkResetPassword: vi.fn(() => Promise.resolve({ total: 1, succeeded: 1, failed: 0, results: [] })),
    usersBulkDelete: vi.fn(() => Promise.resolve({ total: 1, succeeded: 1, failed: 0, results: [] })),
    // 详情 Drawer 数据源(默认空)
    userProfile: vi.fn(() => Promise.resolve({
      userId: 'x', totalRecords: 0, correctRecords: 0, accuracy: null,
      avgResponseTimeMs: null, sessionCount: 0, wordbookDistribution: [],
    })),
    userExtras: vi.fn(() => Promise.resolve(null)),
    userSessions: vi.fn(() => Promise.resolve({ sessions: [] })),
    userDevices: vi.fn(() => Promise.resolve({ devices: [] })),
    userAuditLog: vi.fn(() => Promise.resolve({ entries: [] })),
    userActivityLog: vi.fn(() => Promise.resolve({ entries: [] })),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const baseUser = {
  failedLoginCount: 0,
  lockedUntil: null,
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z',
  role: 'user' as const,
  status: 'active' as const,
  lastLoginAt: null,
  referrerSource: null,
};
const user1 = { ...baseUser, id: '1', username: 'alice', email: 'alice@example.com', isBanned: false };
const user2 = { ...baseUser, id: '2', username: 'bob', email: 'bob@example.com', isBanned: true, status: 'suspended' as const };

const emptyFacets = { total: 0, active: 0, inactive7d: 0, banned: 0, admins: 0 };

function page<T extends object>(data: T[], total = data.length, totalPages = 1) {
  return { data, total, page: 1, perPage: 12, totalPages };
}

async function renderPage() {
  const { default: Page } = await import('@/pages/UserManagementPage');
  return renderWithProviders(() => <Page />);
}

/** facet chip(全部 / 活跃 / 7 天未登录 / 已封禁 / 管理员)是 .badge button —— "管理员"
 *  文案同时出现在角色 select 的 option 里,故取首个(chip)。 */
function clickChip(label: string) {
  const matches = screen.getAllByText(label).filter((el) => el.tagName === 'BUTTON');
  fireEvent.click(matches[0]);
}

describe('UserManagementPage — list, ban, reset password, pagination', () => {
  beforeEach(() => vi.clearAllMocks());

  it('lists users after loading', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1, user2], 2));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    expect(screen.getByText('bob')).toBeInTheDocument();
  });

  it('shows empty state when no users', async () => {
    mockApi.getUsers.mockResolvedValue(page([], 0));
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无用户')).toBeInTheDocument());
  });

  it('renders page head and stays alive when list load fails', async () => {
    // 列表用 createResource(无 catch),失败时不渲染数据/空态,但页面骨架仍在 —— 验证不崩。
    mockApi.getUsers.mockRejectedValue(new Error('500'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('用户管理')).toBeInTheDocument());
    expect(screen.queryByText('alice')).not.toBeInTheDocument();
  });

  it('bans a user via inline action button', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(mockApi.banUser).toHaveBeenCalledWith('1'));
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('opens detail drawer and closes it', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('详情'));
    // Drawer 内有角色变更 select(含「设为管理员」option)
    await waitFor(() => expect(screen.getByText('设为管理员')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('关闭'));
    await waitFor(() => expect(screen.queryByText('设为管理员')).not.toBeInTheDocument());
  });

  it('handles ban failure', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    mockApi.banUser.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('unbans a banned user via inline action button', async () => {
    mockApi.getUsers.mockResolvedValue(page([user2], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('bob')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(mockApi.unbanUser).toHaveBeenCalledWith('2'));
  });

  it('generates reset key from drawer and shows it', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    mockApi.resetUserPassword.mockResolvedValue({ resetKey: 'abc-123', expiresInHours: 24 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('详情'));
    await waitFor(() => expect(screen.getAllByText('重置密码').length).toBeGreaterThan(0));
    fireEvent.click(screen.getAllByText('重置密码')[0]);
    await waitFor(() => expect(screen.getByText('abc-123')).toBeInTheDocument());
    expect(screen.getByText('密码重置密钥')).toBeInTheDocument();
    expect(mockApi.resetUserPassword).toHaveBeenCalledWith('1');
  });

  it('handles generate reset key failure', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    mockApi.resetUserPassword.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('详情'));
    await waitFor(() => expect(screen.getAllByText('重置密码').length).toBeGreaterThan(0));
    fireEvent.click(screen.getAllByText('重置密码')[0]);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('changes role from drawer select', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('详情'));
    await waitFor(() => expect(screen.getByText('设为管理员')).toBeInTheDocument());
    const roleSelect = screen.getByText('设为管理员').closest('select') as HTMLSelectElement;
    fireEvent.change(roleSelect, { target: { value: 'admin' } });
    await waitFor(() => expect(mockApi.patchUserRole).toHaveBeenCalledWith('1', 'admin'));
  });

  it('reloads list with role=user when role select changes', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    // toolbar 顶部「全部角色」select
    const roleFilter = screen.getByText('全部角色').closest('select') as HTMLSelectElement;
    fireEvent.change(roleFilter, { target: { value: 'user' } });
    await waitFor(() =>
      expect(mockApi.getUsers).toHaveBeenLastCalledWith(
        expect.objectContaining({ role: 'user', perPage: 12, includeStats: true }),
      ),
    );
  });

  it('changes pagination page via 下一页', async () => {
    const many = Array.from({ length: 12 }, (_, i) => ({ ...baseUser, id: String(i), username: `u${i}`, email: `${i}@x.com`, isBanned: false }));
    mockApi.getUsers.mockResolvedValue(page(many, 50, 3));
    await renderPage();
    await waitFor(() => expect(screen.getByText('下一页')).toBeInTheDocument());
    fireEvent.click(screen.getByText('下一页'));
    await waitFor(() =>
      expect(mockApi.getUsers).toHaveBeenLastCalledWith(
        expect.objectContaining({ page: 2, perPage: 12, includeStats: true }),
      ),
    );
  });

  it('passes search term into getUsers query (debounced)', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    const searchInput = screen.getByPlaceholderText('搜索邮箱 / 用户名…');
    fireEvent.input(searchInput, { target: { value: 'ali' } });
    await waitFor(
      () =>
        expect(mockApi.getUsers).toHaveBeenLastCalledWith(
          expect.objectContaining({ search: 'ali' }),
        ),
      { timeout: 1500 },
    );
  });
});

describe('UserManagementPage — facet chips & create user', () => {
  beforeEach(() => vi.clearAllMocks());

  it('switches filter chip to "管理员" and re-loads with role=admin', async () => {
    mockApi.userFacets.mockResolvedValue({ total: 100, active: 80, inactive7d: 12, banned: 5, admins: 3 });
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    clickChip('管理员');
    await waitFor(() =>
      expect(mockApi.getUsers).toHaveBeenLastCalledWith(
        expect.objectContaining({ role: 'admin' }),
      ),
    );
  });

  it('switches filter chip to "已封禁" and re-loads with banned=true', async () => {
    mockApi.userFacets.mockResolvedValue({ total: 100, active: 80, inactive7d: 12, banned: 5, admins: 3 });
    mockApi.getUsers.mockResolvedValue(page([user2], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('bob')).toBeInTheDocument());
    clickChip('已封禁');
    await waitFor(() =>
      expect(mockApi.getUsers).toHaveBeenLastCalledWith(
        expect.objectContaining({ banned: true }),
      ),
    );
  });

  it('opens create-user modal and submits', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    mockApi.createUser.mockResolvedValue({ ...baseUser, id: '99', username: 'new', email: 'new@x.com', isBanned: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText('添加用户')).toBeInTheDocument());
    // PageHead 上的「添加用户」按钮
    fireEvent.click(screen.getByText('添加用户'));
    // Modal 打开后,标题「添加用户」会出现两次(按钮 + Modal 标题)
    await waitFor(() => expect(screen.getAllByText('添加用户').length).toBeGreaterThanOrEqual(2));
    // Modal 内三个输入按 placeholder 精确定位(避免误中顶部搜索框)
    const email = screen.getByPlaceholderText('user@example.com') as HTMLInputElement;
    const usernameInput = screen.getByPlaceholderText('2-50 字符,字母数字下划线') as HTMLInputElement;
    const pwInput = screen.getByPlaceholderText('至少 8 字符,含大小写字母 + 数字') as HTMLInputElement;
    fireEvent.input(email, { target: { value: 'new@x.com' } });
    fireEvent.input(usernameInput, { target: { value: 'newuser' } });
    fireEvent.input(pwInput, { target: { value: 'NewPass123' } });
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() =>
      expect(mockApi.createUser).toHaveBeenCalledWith(
        expect.objectContaining({ email: 'new@x.com', username: 'newuser', password: 'NewPass123' }),
      ),
    );
  });

  it('warns when create-user form incomplete', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('添加用户')).toBeInTheDocument());
    fireEvent.click(screen.getByText('添加用户'));
    await waitFor(() => expect(screen.getAllByText('添加用户').length).toBeGreaterThanOrEqual(2));
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() => expect(mockToast.warning).toHaveBeenCalled());
    expect(mockApi.createUser).not.toHaveBeenCalled();
  });
});

describe('UserManagementPage — bulk actions & reset-key copy', () => {
  beforeEach(() => vi.clearAllMocks());

  it('selecting a row reveals the bulk bar and bulk-bans via confirm', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    const checkboxes = document.querySelectorAll('input[type="checkbox"]');
    // [0] = 全选, [1] = 第一行
    fireEvent.change(checkboxes[1], { target: { checked: true } });
    await waitFor(() => expect(screen.getByText('已选 1 个')).toBeInTheDocument());
    // bulk bar 「封禁」是 btn-secondary(行内封禁是 btn-danger)—— 取批量栏那个
    const bulkBan = screen.getAllByText('封禁').find((b) => b.className.includes('btn-secondary'))!;
    fireEvent.click(bulkBan);
    // Confirm 弹窗
    await waitFor(() => expect(screen.getByText('批量封禁 1 个用户')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认'));
    await waitFor(() => expect(mockApi.usersBulkBan).toHaveBeenCalledWith(['1']));
  });

  it('cancels the bulk confirm dialog', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    const checkboxes = document.querySelectorAll('input[type="checkbox"]');
    fireEvent.change(checkboxes[1], { target: { checked: true } });
    await waitFor(() => expect(screen.getByText('已选 1 个')).toBeInTheDocument());
    const bulkBan = screen.getAllByText('封禁').find((b) => b.className.includes('btn-secondary'))!;
    fireEvent.click(bulkBan);
    await waitFor(() => expect(screen.getByText('批量封禁 1 个用户')).toBeInTheDocument());
    // 「取消」同时存在于批量栏与 Confirm footer —— 取 Confirm 弹窗(.card.scale-in)内的那个
    const confirmCard = screen.getByText('批量封禁 1 个用户').closest('.card') as HTMLElement;
    const cancelBtn = Array.from(confirmCard.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === '取消',
    )!;
    fireEvent.click(cancelBtn);
    await waitFor(() => expect(screen.queryByText('批量封禁 1 个用户')).not.toBeInTheDocument());
    expect(mockApi.usersBulkBan).not.toHaveBeenCalled();
  });

  it('copy button on reset-key modal calls clipboard.writeText', async () => {
    mockApi.getUsers.mockResolvedValue(page([user1], 1));
    mockApi.resetUserPassword.mockResolvedValue({ resetKey: 'XYZ-789', expiresInHours: 24 });
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    await renderPage();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('详情'));
    await waitFor(() => expect(screen.getAllByText('重置密码').length).toBeGreaterThan(0));
    fireEvent.click(screen.getAllByText('重置密码')[0]);
    await waitFor(() => expect(screen.getByText('XYZ-789')).toBeInTheDocument());
    fireEvent.click(screen.getByText('复制密钥'));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('XYZ-789'));
    expect(mockToast.success).toHaveBeenCalledWith('已复制');
  });
});
