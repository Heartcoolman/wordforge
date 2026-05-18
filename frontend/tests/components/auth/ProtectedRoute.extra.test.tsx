import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getAdminToken: vi.fn(() => 'tok'),
    clearAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/admin', () => ({
  adminApi: { verifyToken: vi.fn() },
}));

import { tokenManager } from '@/lib/token';
import { adminApi } from '@/api/admin';

beforeEach(() => vi.clearAllMocks());

describe('AdminProtectedRoute (extra branches)', () => {
  it('clears token and redirects when verifyToken fails on mount', async () => {
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue('tok');
    (adminApi.verifyToken as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('401'));

    const { AdminProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <AdminProtectedRoute>Hidden</AdminProtectedRoute>);

    await waitFor(() => {
      expect(tokenManager.clearAdminToken).toHaveBeenCalled();
    });
  });

  it('handles focus event re-verification failure', async () => {
    (tokenManager.getAdminToken as ReturnType<typeof vi.fn>).mockReturnValue('tok');
    (adminApi.verifyToken as ReturnType<typeof vi.fn>).mockResolvedValue({ id: '1', email: 'a@b.c' });

    const { AdminProtectedRoute } = await import('@/components/auth/ProtectedRoute');
    renderWithProviders(() => <AdminProtectedRoute>Body</AdminProtectedRoute>);
    await waitFor(() => expect(screen.getByText('Body')).toBeInTheDocument());

    // 通过 Date.now stub 越过 30s 节流
    const realNow = Date.now;
    Date.now = () => realNow() + 60_000;
    try {
      (adminApi.verifyToken as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('expired'));
      window.dispatchEvent(new Event('focus'));
      await waitFor(() => {
        expect(tokenManager.clearAdminToken).toHaveBeenCalled();
      });
    } finally {
      Date.now = realNow;
    }
  });
});
