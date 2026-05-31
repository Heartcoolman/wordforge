import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import type { AdminUpdateStatus, ChannelStatus } from '@/types/admin';

let lastSseHandlers: any = null;

vi.mock('@/api/admin', () => ({
  adminApi: {
    updatesStatus: vi.fn(),
    updatesCheck: vi.fn(),
    updatesApply: vi.fn(),
    updatesHistory: vi.fn(() => Promise.resolve({ entries: [] })),
    getSettings: vi.fn(() => Promise.resolve({ maintenanceMode: false })),
    setMaintenance: vi.fn(),
    getHealth: vi.fn(() => Promise.resolve({ status: 'healthy' })),
    updatesBackups: vi.fn(() => Promise.resolve({ backups: [], totalBytes: 0, thresholdBytes: 10_737_418_240 })),
    updatesChangelog: vi.fn(() => Promise.resolve({ available: false })),
    updatesCreateBackup: vi.fn(),
    updatesRestoreBackup: vi.fn(),
    updatesBackupDownloadUrl: vi.fn(),
    updatesRollback: vi.fn(),
  },
}));
vi.mock('@/api/http', async () => {
  const actual = await vi.importActual<typeof import('@/api/http')>('@/api/http');
  return {
    ...actual,
    connectSseStream: vi.fn((handlers: any) => {
      lastSseHandlers = handlers;
      return () => {};
    }),
  };
});
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

/// v0.6.0-beta.3：双通道嵌套 fixture。stable 卡有 1.1.0 升级；beta=null。
const baseStatus: AdminUpdateStatus = {
  currentVersion: '1.0.0',
  stable: {
    latestVersion: '1.1.0',
    latestPublishedAt: '2026-04-15T10:00:00Z',
    releaseNotes: '修复若干 bug',
    releaseUrl: 'https://github.com/x/y/releases/v1.1.0',
    hasUpdate: true,
    canApply: true,
    tarballSize: 15523840,
    sha256: 'a4f2c1d9e8b7a6f5b9c1',
  },
  beta: null,
  autoCheckEnabled: true,
  lastCheckedAt: '2026-04-15T10:00:00Z',
  allowDowngrade: false,
  installedAt: '2026-04-01T00:00:00Z',
  uptimeSecs: 7200,
};
const betaChannel: ChannelStatus = {
  latestVersion: 'v0.6.0-beta.3',
  latestPublishedAt: '2026-05-20T20:17:07Z',
  releaseNotes: 'beta notes',
  releaseUrl: 'https://example/r/v0.6.0-beta.3',
  hasUpdate: true,
  canApply: true,
  tarballSize: 14000000,
  sha256: 'beefcafe1234',
};

async function renderPage() {
  const { default: Page } = await import('@/pages/UpdatesPage');
  return renderWithProviders(() => <Page />);
}

describe('UpdatesPage — status, check, apply', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lastSseHandlers = null;
  });

  it('renders hero version card and stable channel release notes', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByText('当前版本')).toBeInTheDocument());
    expect(screen.getAllByText('1.0.0').length).toBeGreaterThan(0);
    expect(screen.getByText('Release Notes')).toBeInTheDocument();
    // release notes 在通道详情卡与 CHANGELOG 回退区各渲染一次，断言至少一处
    expect(screen.getAllByText('修复若干 bug').length).toBeGreaterThan(0);
  });

  it('shows nil channel state when stable channel is null', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, stable: null, lastCheckedAt: null });
    await renderPage();
    // stable=null → 通道详情卡显示「尚未检查」
    await waitFor(() => expect(screen.getByText('尚未检查')).toBeInTheDocument());
  });

  it('shows arch warning when hasUpdate but canApply is false', async () => {
    mockApi.updatesStatus.mockResolvedValue({
      ...baseStatus,
      stable: { ...baseStatus.stable!, canApply: false },
    });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/未找到匹配当前架构的产物/)).toBeInTheDocument());
  });

  it('triggers check and shows update found toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '立即检查' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '立即检查' }));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('triggers check and shows up-to-date toast when no channel has update', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue({
      ...baseStatus,
      stable: { ...baseStatus.stable!, hasUpdate: false, canApply: false },
    });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '立即检查' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '立即检查' }));
    await waitFor(() => expect(mockToast.info).toHaveBeenCalled());
  });

  it('shows check failure toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockRejectedValue(new Error('net'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '立即检查' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '立即检查' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('opens confirm dialog and applies via 开始升级', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesApply.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /立即升级到 1\.1\.0/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /立即升级到 1\.1\.0/ }));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('开始升级'));
    await waitFor(() => expect(mockApi.updatesApply).toHaveBeenCalled());
  });

  it('cancels apply confirm dialog', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /立即升级到 1\.1\.0/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: /立即升级到 1\.1\.0/ }));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
  });
});

describe('UpdatesPage — Beta channel folding + cross-channel apply', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lastSseHandlers = null;
  });

  it('renders Beta collapsible with badge when beta has update', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, beta: betaChannel });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /Beta 通道/ })).toBeInTheDocument());
    // 折叠条上的 badge 显示 beta latestVersion
    expect(screen.getAllByText('v0.6.0-beta.3').length).toBeGreaterThan(0);
  });

  it('expands Beta region on click and shows upgrade button', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, beta: betaChannel });
    await renderPage();
    const trigger = await screen.findByRole('button', { name: /Beta 通道/ });
    fireEvent.click(trigger);
    await waitFor(() => {
      const buttons = screen.getAllByRole('button', { name: /一键升级到 v0\.6\.0-beta\.3/ });
      expect(buttons.length).toBeGreaterThan(0);
    });
  });

  it('passes channel="beta" to updatesApply when applying from Beta card', async () => {
    const status: AdminUpdateStatus = {
      ...baseStatus,
      // stable 没有更新，仅 beta 有 → hero 走 beta；beta 折叠区也可点
      stable: { ...baseStatus.stable!, hasUpdate: false, canApply: false, latestVersion: '1.0.0' },
      beta: betaChannel,
    };
    mockApi.updatesStatus.mockResolvedValue(status);
    mockApi.updatesApply.mockResolvedValue(undefined);
    await renderPage();
    const trigger = await screen.findByRole('button', { name: /Beta 通道/ });
    fireEvent.click(trigger);
    // 通道卡按钮用「一键升级到」前缀，与 hero 的「立即升级到」区分
    const upgradeBtn = await screen.findByRole('button', {
      name: /一键升级到 v0\.6\.0-beta\.3/,
    });
    fireEvent.click(upgradeBtn);
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('开始升级'));
    await waitFor(() =>
      expect(mockApi.updatesApply).toHaveBeenCalledWith('beta', 'v0.6.0-beta.3', '1.0.0'),
    );
  });

  it('hides badge when beta has no update', async () => {
    mockApi.updatesStatus.mockResolvedValue({
      ...baseStatus,
      beta: { ...betaChannel, releaseNotes: '', releaseUrl: '', hasUpdate: false, canApply: false },
    });
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /Beta 通道/ })).toBeInTheDocument());
    const trigger = screen.getByRole('button', { name: /Beta 通道/ });
    expect(trigger.textContent).not.toMatch(/v0\.6\.0-beta\.3/);
  });
});

describe('UpdatesPage — SSE handlers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    lastSseHandlers = null;
  });

  it('invokes onReleaseAvailable with channel: info toast + refetch', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(lastSseHandlers).not.toBeNull());
    expect(typeof lastSseHandlers.onReleaseAvailable).toBe('function');
    // v0.6.0-beta.3：payload 含 channel
    lastSseHandlers.onReleaseAvailable({ latestTag: 'v0.6.0-beta.3', channel: 'beta' });
    await waitFor(() => {
      const calls = mockToast.info.mock.calls;
      const hit = calls.some((c) => typeof c[0] === 'string' && c[0].includes('Beta 通道'));
      expect(hit).toBe(true);
    });
  });

  it('invokes onUpdateProgress: sets progress state without throwing', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(lastSseHandlers).not.toBeNull());
    expect(typeof lastSseHandlers.onUpdateProgress).toBe('function');
    expect(() => lastSseHandlers.onUpdateProgress({ phase: 'download', percent: 42 })).not.toThrow();
  });
});
