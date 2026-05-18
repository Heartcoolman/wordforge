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

// ApiError 是 client.ts 实现的 class；测试里需要 instanceof 校验，
// 所以保留真 class，只 mock connectSseStream
vi.mock('@/api/client', async () => {
  const actual = await vi.importActual<typeof import('@/api/client')>('@/api/client');
  return {
    ...actual,
    connectSseStream: vi.fn(() => () => {}),
  };
});

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

  it('shows server-side error toast when apply returns 422 SHA256_MISMATCH', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    const { ApiError } = await import('@/api/client');
    const uiStoreMod = await import('@/stores/ui');
    const errMock = uiStoreMod.uiStore.toast.error as ReturnType<typeof vi.fn>;
    mockAdminApi.updatesApply.mockRejectedValue(
      new ApiError(422, 'SHA256_MISMATCH', 'sha256 mismatch: expected ... got ...'),
    );
    await renderPage();
    await waitFor(() => expect(screen.getByText('v0.4.3')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /一键更新到/ }));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '开始升级' }));
    await waitFor(() => {
      expect(errMock).toHaveBeenCalled();
      const calls = errMock.mock.calls;
      const matched = calls.some(([title]) =>
        typeof title === 'string' && title.includes('SHA256_MISMATCH'),
      );
      expect(matched).toBe(true);
    });
    // 4xx 时应停在 error 分支后的 refetch 一次，不进 2 分钟 polling 循环；
    // 初始 resource + refetch = ≤ 2 次，绝不会到 polling 的多次调用
    expect(mockAdminApi.updatesStatus.mock.calls.length).toBeLessThanOrEqual(2);
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
