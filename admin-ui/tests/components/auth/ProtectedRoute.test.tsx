import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getAdminToken: vi.fn(() => null),
    clearAdminToken: vi.fn(),
  },
}));

vi.mock('@/api/admin', () => ({
  adminApi: {
    verifyToken: vi.fn().mockResolvedValue({ id: 'admin-1', email: 'admin@test.com' }),
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe('AdminProtectedRoute', () => {
  it('shows spinner initially', async () => {
    const { AdminProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <AdminProtectedRoute>Admin</AdminProtectedRoute>);
    expect(document.body).toBeInTheDocument();
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
  });

  it('redirects away when no admin token', async () => {
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

    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue(null);
    window.dispatchEvent(new Event('admin:unauthorized'));

    await waitFor(() => {
      expect(screen.queryByText('Admin')).not.toBeInTheDocument();
    });
  });
});
