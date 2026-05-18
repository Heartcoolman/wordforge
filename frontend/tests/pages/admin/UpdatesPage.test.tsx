import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    updatesStatus: vi.fn(),
    updatesCheck: vi.fn(),
    updatesApply: vi.fn(),
  },
}));

vi.mock('@/api/client', () => ({
  connectSseStream: vi.fn(() => () => {}),
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockStatusHasUpdate = {
  currentVersion: 'v0.4.2',
  latestVersion: 'v0.4.3',
  latestPublishedAt: '2026-05-17T16:00:00Z',
  releaseNotes: '## Changelog\n- bug fix',
  releaseUrl: 'https://github.com/Heartcoolman/wordforge/releases/tag/v0.4.3',
  hasUpdate: true,
  canApply: true,
  lastCheckedAt: '2026-05-17T16:05:00Z',
  autoCheckEnabled: true,
  allowDowngrade: false,
};

const mockStatusNoUpdate = {
  ...mockStatusHasUpdate,
  latestVersion: 'v0.4.2',
  hasUpdate: false,
  canApply: false,
};

describe('UpdatesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: UpdatesPage } = await import('@/pages/admin/UpdatesPage');
    return renderWithProviders(() => <UpdatesPage />);
  }

  it('shows current and latest version after loading', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('v0.4.2')).toBeInTheDocument();
    });
    expect(screen.getByText('v0.4.3')).toBeInTheDocument();
  });

  it('shows release notes when present', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('Release Notes')).toBeInTheDocument();
    });
  });

  it('disables apply button when no update available', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusNoUpdate);
    await renderPage();
    await waitFor(() => {
      const btn = screen.getByRole('button', { name: /一键更新到/ });
      expect(btn).toBeDisabled();
    });
  });

  it('opens confirm modal on apply click', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('v0.4.3')).toBeInTheDocument();
    });
    const applyBtn = screen.getByRole('button', { name: /一键更新到/ });
    fireEvent.click(applyBtn);
    await waitFor(() => {
      expect(screen.getByText('确认一键更新')).toBeInTheDocument();
    });
  });

  it('triggers updatesCheck on 立即检查 click', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    mockAdminApi.updatesCheck.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('v0.4.3')).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: '立即检查' }));
    await waitFor(() => {
      expect(mockAdminApi.updatesCheck).toHaveBeenCalled();
    });
  });
});
