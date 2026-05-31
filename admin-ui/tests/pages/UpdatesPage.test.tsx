import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import type { AdminUpdateStatus } from '@/types/admin';

vi.mock('@/api/admin', () => ({
  adminApi: {
    updatesStatus: vi.fn(),
    updatesCheck: vi.fn(),
    updatesApply: vi.fn(),
    updatesHistory: vi.fn(() => Promise.resolve({ entries: [] })),
    getSettings: vi.fn(() => Promise.resolve({ maintenanceMode: false })),
    setMaintenance: vi.fn(),
    // 版本页对齐设计稿新增 resource —— 全部给安全桩，避免 createResource 调 undefined
    getHealth: vi.fn(() => Promise.resolve({ status: 'healthy' })),
    updatesBackups: vi.fn(() => Promise.resolve({ backups: [], totalBytes: 0, thresholdBytes: 10_737_418_240 })),
    updatesChangelog: vi.fn(() => Promise.resolve({ available: false })),
    updatesCreateBackup: vi.fn(),
    updatesRestoreBackup: vi.fn(),
    updatesBackupDownloadUrl: vi.fn(),
    updatesRollback: vi.fn(),
  },
}));

// ApiError 是 client.ts 实现的 class；测试里需要 instanceof 校验，
// 所以保留真 class，只 mock connectSseStream
vi.mock('@/api/http', async () => {
  const actual = await vi.importActual<typeof import('@/api/http')>('@/api/http');
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

/// v0.6.0-beta.3：UpdateStatus 拆双通道嵌套。stable 卡有更新 + beta=null 的典型 fixture。
const mockStatusHasUpdate: AdminUpdateStatus = {
  currentVersion: 'v0.4.2',
  stable: {
    latestVersion: 'v0.4.3',
    latestPublishedAt: '2026-05-17T16:00:00Z',
    releaseNotes: '## Changelog\n- bug fix',
    releaseUrl: 'https://github.com/Heartcoolman/wordforge/releases/tag/v0.4.3',
    hasUpdate: true,
    canApply: true,
    tarballSize: 15523840,
    sha256: 'a4f2c1d9e8b7a6f5b9c1',
  },
  beta: null,
  lastCheckedAt: '2026-05-17T16:05:00Z',
  autoCheckEnabled: true,
  allowDowngrade: false,
  installedAt: '2026-05-01T00:00:00Z',
  uptimeSecs: 3600,
};

const mockStatusNoUpdate: AdminUpdateStatus = {
  ...mockStatusHasUpdate,
  stable: {
    ...mockStatusHasUpdate.stable!,
    latestVersion: 'v0.4.2',
    hasUpdate: false,
    canApply: false,
  },
};

describe('UpdatesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: UpdatesPage } = await import('@/pages/UpdatesPage');
    return renderWithProviders(() => <UpdatesPage />);
  }

  it('shows current and stable-channel latest version after loading', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    // 当前版本在版本卡 .v / .vv 多处出现，用 getAllByText 断言至少一处
    await waitFor(() => {
      expect(screen.getAllByText('v0.4.2').length).toBeGreaterThan(0);
    });
    expect(screen.getAllByText('v0.4.3').length).toBeGreaterThan(0);
  });

  it('shows release notes label in stable channel detail', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('Release Notes')).toBeInTheDocument();
    });
  });

  it('disables stable apply button when stable channel has no update', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusNoUpdate);
    await renderPage();
    await waitFor(() => {
      const btn = screen.getByRole('button', { name: /已是最新/ });
      expect(btn).toBeDisabled();
    });
  });

  it('opens confirm modal on hero upgrade click', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /立即升级到 v0\.4\.3/ })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: /立即升级到 v0\.4\.3/ }));
    await waitFor(() => {
      expect(screen.getByText('确认一键更新')).toBeInTheDocument();
    });
  });

  it('shows server-side error toast when apply returns 422 SHA256_MISMATCH', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    const { ApiError } = await import('@/api/http');
    const uiStoreMod = await import('@/stores/ui');
    const errMock = uiStoreMod.uiStore.toast.error as ReturnType<typeof vi.fn>;
    mockAdminApi.updatesApply.mockRejectedValue(
      new ApiError(422, 'SHA256_MISMATCH', 'sha256 mismatch: expected ... got ...'),
    );
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /立即升级到 v0\.4\.3/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /立即升级到 v0\.4\.3/ }));
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
    // 4xx 时应停在 error 分支后的 refetch，不进 polling 循环（polling 会多次调 updatesStatus）
    expect(mockAdminApi.updatesStatus.mock.calls.length).toBeLessThanOrEqual(3);
  });

  it('passes channel="stable" to updatesApply when applying from hero', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    mockAdminApi.updatesApply.mockResolvedValue({ taskId: 't1', phase: 'pending', percent: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /立即升级到 v0\.4\.3/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /立即升级到 v0\.4\.3/ }));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '开始升级' }));
    await waitFor(() => {
      expect(mockAdminApi.updatesApply).toHaveBeenCalledWith('stable', 'v0.4.3', 'v0.4.2');
    });
  });

  it('triggers updatesCheck on 立即检查 click', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    mockAdminApi.updatesCheck.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '立即检查' })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: '立即检查' }));
    await waitFor(() => {
      expect(mockAdminApi.updatesCheck).toHaveBeenCalled();
    });
  });

  it('renders backup row and triggers create backup', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    mockAdminApi.updatesBackups.mockResolvedValue({
      backups: [
        { name: 'backup-manual-20251125-214700.db', kind: 'manual', sizeBytes: 412 * 1024 * 1024, createdAt: '2025-11-25T21:47:00Z', version: null },
      ],
      totalBytes: 412 * 1024 * 1024,
      thresholdBytes: 10_737_418_240,
    });
    mockAdminApi.updatesCreateBackup.mockResolvedValue({
      name: 'backup-manual-x.db', kind: 'manual', sizeBytes: 1, createdAt: '2025-11-26T00:00:00Z', version: null,
    });
    await renderPage();
    // "SQLite 备份" 在备份面板标题等多处出现，断言至少一处
    await waitFor(() => expect(screen.getAllByText('SQLite 备份').length).toBeGreaterThan(0));
    expect(screen.getByText('backup-manual-20251125-214700.db')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '立即备份' }));
    await waitFor(() => expect(mockAdminApi.updatesCreateBackup).toHaveBeenCalled());
  });
});
