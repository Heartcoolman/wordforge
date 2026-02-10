import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getUsers: vi.fn(),
    banUser: vi.fn(),
    unbanUser: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockUsers = [
  { id: '1', username: 'alice', email: 'alice@test.com', isBanned: false },
  { id: '2', username: 'bob', email: 'bob@test.com', isBanned: true },
];

describe('UserManagementPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: UserManagementPage } = await import('@/pages/admin/UserManagementPage');
    return renderWithProviders(() => <UserManagementPage />);
  }

  it('shows "用户管理" heading', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    expect(screen.getByText('用户管理')).toBeInTheDocument();
  });

  it('shows loading spinner initially', async () => {
    mockAdminApi.getUsers.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows user table with usernames after loading', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('alice')).toBeInTheDocument();
    });
    expect(screen.getByText('bob')).toBeInTheDocument();
  });

  it('shows table column headers', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('用户名')).toBeInTheDocument();
    });
    expect(screen.getByText('邮箱')).toBeInTheDocument();
    expect(screen.getByText('状态')).toBeInTheDocument();
    expect(screen.getByText('操作')).toBeInTheDocument();
  });

  it('shows "封禁" button for active users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('封禁')).toBeInTheDocument();
    });
  });

  it('shows "解封" button for banned users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('解封')).toBeInTheDocument();
    });
  });

  it('shows status badges for users', async () => {
    mockAdminApi.getUsers.mockResolvedValue(mockUsers);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('正常')).toBeInTheDocument();
    });
    expect(screen.getByText('已封禁')).toBeInTheDocument();
  });
});
