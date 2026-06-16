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
    // 版本页对齐设计稿的 resource —— 全部给安全桩，避免 createResource 调 undefined
    getHealth: vi.fn(() => Promise.resolve({ status: 'healthy' })),
    updatesBackups: vi.fn(() => Promise.resolve({ backups: [], totalBytes: 0, thresholdBytes: 10_737_418_240 })),
    updatesChangelog: vi.fn(() => Promise.resolve({ available: false })),
    updatesCreateBackup: vi.fn(),
    updatesRestoreBackup: vi.fn(),
    updatesBackupDownloadUrl: vi.fn(),
    updatesRollback: vi.fn(),
  },
}));

// ApiError 是 http.ts 实现的 class；测试里需要 instanceof 校验，
// 保留真 class，只 mock connectSseStream
vi.mock('@/api/http', async () => {
  const actual = await vi.importActual<typeof import('@/api/http')>('@/api/http');
  return {
    ...actual,
    connectSseStream: vi.fn(() => () => {}),
  };
});

// 新页面通过 @/components/wf 的 toast（= uiStore.toast）报错/提示
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

// stable 卡有更新 + beta=null 的典型 fixture（双通道嵌套）。canApply=true 让一键升级按钮可点。
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
    // 当前版本 + 目标版本在升级流水线卡片渲染
    await waitFor(() => {
      expect(screen.getAllByText('v0.4.2').length).toBeGreaterThan(0);
    });
    expect(screen.getAllByText('v0.4.3').length).toBeGreaterThan(0);
  });

  it('renders the upgrade pipeline and CHANGELOG sections', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('升级流水线')).toBeInTheDocument();
    });
    expect(screen.getByText('CHANGELOG')).toBeInTheDocument();
    // changelog 不可用时回退渲染当前通道 releaseNotes
    expect(screen.getByText(/bug fix/)).toBeInTheDocument();
  });

  it('disables the one-click upgrade button when stable channel has no update', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusNoUpdate);
    await renderPage();
    await waitFor(() => {
      const btn = screen.getByRole('button', { name: /一键升级到 v0\.4\.2/ });
      expect(btn).toBeDisabled();
    });
  });

  it('opens confirm modal on one-click upgrade click', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /一键升级到 v0\.4\.3/ })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: /一键升级到 v0\.4\.3/ }));
    await waitFor(() => {
      expect(screen.getByText('确认一键更新')).toBeInTheDocument();
    });
  });

  it('shows error toast when apply returns 422 SHA256_MISMATCH', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    const { ApiError } = await import('@/api/http');
    const uiStoreMod = await import('@/stores/ui');
    const errMock = uiStoreMod.uiStore.toast.error as ReturnType<typeof vi.fn>;
    mockAdminApi.updatesApply.mockRejectedValue(
      new ApiError(422, 'SHA256_MISMATCH', 'sha256 mismatch: expected ... got ...'),
    );
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /一键升级到 v0\.4\.3/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /一键升级到 v0\.4\.3/ }));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '开始升级' }));
    await waitFor(() => {
      expect(errMock).toHaveBeenCalled();
      // doApply 的 ApiError 分支把 code 拼进 detail：`[SHA256_MISMATCH] ...`
      const matched = errMock.mock.calls.some(([, detail]) =>
        typeof detail === 'string' && detail.includes('SHA256_MISMATCH'),
      );
      expect(matched).toBe(true);
    });
    // 4xx 失败应停在 error 分支，不进 polling 循环（无 applyTask → createEffect 早退）
    expect(mockAdminApi.updatesStatus.mock.calls.length).toBeLessThanOrEqual(3);
  });

  it('passes channel="stable" + versions to updatesApply when confirming', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    mockAdminApi.updatesApply.mockResolvedValue({
      taskId: 't1', phase: 'pending', percent: 0, targetVersion: 'v0.4.3', startedAt: '2026-05-17T16:06:00Z',
    });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /一键升级到 v0\.4\.3/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /一键升级到 v0\.4\.3/ }));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '开始升级' }));
    await waitFor(() => {
      expect(mockAdminApi.updatesApply).toHaveBeenCalledWith('stable', 'v0.4.3', 'v0.4.2');
    });
  });

  it('triggers updatesCheck on 检查更新 click', async () => {
    mockAdminApi.updatesStatus.mockResolvedValue(mockStatusHasUpdate);
    mockAdminApi.updatesCheck.mockResolvedValue(mockStatusHasUpdate);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '检查更新' })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: '检查更新' }));
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
    // 备份面板标题为「数据库备份」，列表渲染备份文件名
    await waitFor(() => expect(screen.getByText('数据库备份')).toBeInTheDocument());
    expect(screen.getByText('backup-manual-20251125-214700.db')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '立即备份' }));
    await waitFor(() => expect(mockAdminApi.updatesCreateBackup).toHaveBeenCalled());
  });
});
