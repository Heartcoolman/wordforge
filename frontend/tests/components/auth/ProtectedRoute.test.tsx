import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/stores/auth', () => ({
  authStore: {
    loading: vi.fn(() => false),
    isAuthenticated: vi.fn(() => false),
    initialized: vi.fn(() => true),
    init: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getAdminToken: vi.fn(() => null),
    clearAdminToken: vi.fn(),
    clearTokens: vi.fn(),
    getToken: vi.fn(() => null),
    isAuthenticated: vi.fn(() => false),
    needsRefresh: vi.fn(() => false),
    refreshAccessToken: vi.fn().mockResolvedValue(false),
  },
}));

vi.mock('@/api/admin', () => ({
  adminApi: {
    verifyToken: vi.fn().mockResolvedValue({ id: 'admin-1', email: 'admin@test.com' }),
  },
}));

vi.mock('@/api/users', () => ({
  usersApi: {
    getMe: vi.fn().mockResolvedValue({ id: 'user-1', username: 'test' }),
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe('ProtectedRoute', () => {
  it('shows spinner when loading', async () => {
    const { authStore } = await import('@/stores/auth');
    (authStore.loading as ReturnType<typeof vi.fn>).mockReturnValue(true);
    (authStore.initialized as ReturnType<typeof vi.fn>).mockReturnValue(true);
    const { ProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <ProtectedRoute>Secret</ProtectedRoute>);
    expect(screen.getByRole('status')).toBeInTheDocument();
    (authStore.loading as ReturnType<typeof vi.fn>).mockReturnValue(false);
  });

  it('shows children when authenticated and verified', async () => {
    const { authStore } = await import('@/stores/auth');
    const { usersApi } = await import('@/api/users');
    (authStore.loading as ReturnType<typeof vi.fn>).mockReturnValue(false);
    (authStore.isAuthenticated as ReturnType<typeof vi.fn>).mockReturnValue(true);
    (authStore.initialized as ReturnType<typeof vi.fn>).mockReturnValue(true);
    (usersApi.getMe as ReturnType<typeof vi.fn>).mockResolvedValue({ id: 'user-1', username: 'test' });
    const { ProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <ProtectedRoute>Secret Content</ProtectedRoute>);
    await waitFor(() => {
      expect(screen.getByText('Secret Content')).toBeInTheDocument();
    });
    (authStore.isAuthenticated as ReturnType<typeof vi.fn>).mockReturnValue(false);
  });

  it('redirects to /login when not authenticated', async () => {
    const { authStore } = await import('@/stores/auth');
    (authStore.loading as ReturnType<typeof vi.fn>).mockReturnValue(false);
    (authStore.isAuthenticated as ReturnType<typeof vi.fn>).mockReturnValue(false);
    (authStore.initialized as ReturnType<typeof vi.fn>).mockReturnValue(true);
    const { ProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <ProtectedRoute>Secret</ProtectedRoute>);
    await waitFor(() => {
      expect(screen.queryByText('Secret')).not.toBeInTheDocument();
    });
  });
});

describe('AdminProtectedRoute', () => {
  it('shows spinner initially', async () => {
    const { tokenManager } = await import('@/lib/token');
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue(null);
    const { AdminProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <AdminProtectedRoute>Admin</AdminProtectedRoute>);
    await waitFor(() => {
      expect(document.body).toBeInTheDocument();
    });
  });

  it('shows children when admin token exists and verifyToken succeeds', async () => {
    const { tokenManager } = await import('@/lib/token');
    const { adminApi } = await import('@/api/admin');
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue('admin-token');
    (adminApi.verifyToken as ReturnType<typeof vi.fn>).mockResolvedValue({ id: 'admin-1', email: 'admin@test.com' });
    const { AdminProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <AdminProtectedRoute>Admin Panel</AdminProtectedRoute>);
    await waitFor(() => {
      expect(screen.getByText('Admin Panel')).toBeInTheDocument();
    });
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue(null);
  });

  it('redirects to /admin/login when no admin token', async () => {
    const { tokenManager } = await import('@/lib/token');
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue(null);
    const { AdminProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <AdminProtectedRoute>Admin</AdminProtectedRoute>);
    await waitFor(() => {
      expect(screen.queryByText('Admin')).not.toBeInTheDocument();
    });
  });

  it('handles admin:unauthorized event by removing content', async () => {
    const { tokenManager } = await import('@/lib/token');
    const { adminApi } = await import('@/api/admin');
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue('token');
    (adminApi.verifyToken as ReturnType<typeof vi.fn>).mockResolvedValue({ id: 'admin-1', email: 'admin@test.com' });
    const { AdminProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <AdminProtectedRoute>Admin</AdminProtectedRoute>);
    await waitFor(() => {
      expect(screen.getByText('Admin')).toBeInTheDocument();
    });
    // 模拟 admin:unauthorized 事件（例如后端返回 401 时触发）
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue(null);
    window.dispatchEvent(new Event('admin:unauthorized'));
    await waitFor(() => {
      expect(screen.queryByText('Admin')).not.toBeInTheDocument();
    });
  });
});
