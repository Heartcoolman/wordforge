import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import type { AdminUpdateStatus, ChannelStatus } from '@/types/admin';

// 重设计后的 UpdatesPage 调用 adminApi 的这些方法（无 SSE）。
// 每个 mock 都给安全默认 resolved value，避免 createResource 未处理拒绝。
vi.mock('@/api/admin', () => ({
  adminApi: {
    updatesStatus: vi.fn(),
    updatesCheck: vi.fn(),
    updatesApply: vi.fn(() => Promise.resolve(undefined)),
    updatesRollback: vi.fn(() => Promise.resolve(undefined)),
    updatesHistory: vi.fn(() => Promise.resolve({ entries: [] })),
    updatesChangelog: vi.fn(() => Promise.resolve({ available: false })),
    updatesBackups: vi.fn(() =>
      Promise.resolve({ backups: [], totalBytes: 0, thresholdBytes: 10_737_418_240 }),
    ),
    updatesCreateBackup: vi.fn(() => Promise.resolve({ name: 'manual-x.db', kind: 'manual', sizeBytes: 1024, createdAt: '2026-04-15T10:00:00Z', version: '1.0.0' })),
    updatesRestoreBackup: vi.fn(() => Promise.resolve({ restored: true, restartRecommended: true, preRestoreBackup: 'pre.db' })),
    updatesBackupDownloadUrl: vi.fn(() => Promise.resolve('blob:x')),
    getSettings: vi.fn(() => Promise.resolve({ maintenanceMode: false })),
    setMaintenance: vi.fn(() => Promise.resolve({ active: true })),
    getHealth: vi.fn(() => Promise.resolve({ status: 'healthy', dbSizeBytes: 0, uptimeSecs: 0, version: '1.0.0', errorRate: 0 })),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

// 双通道嵌套 fixture：stable 卡有 1.1.0 可升级；beta=null。
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

// 选中 Beta 通道：升级流水线里的 Seg 段「Beta 通道」标签按钮。
// 该 Seg 包在 <Field label="升级通道"> 内，dom-accessibility-api 计算出的无障碍名
// 把 label 串入且与可见文本错位，按 role+name 会命中错误按钮；故直接按可见文本
// （Seg 渲染为 class="tab" 的 <button>，textContent 即「Beta 通道」）定位点击。
async function selectBeta() {
  const seg = await screen.findByText('Beta 通道');
  fireEvent.click(seg);
}

describe('UpdatesPage — 升级流水线、检查、升级', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('渲染升级流水线卡：当前版本与 CHANGELOG 回退的 release notes', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByText('升级流水线')).toBeInTheDocument());
    // 当前版本号 1.0.0 出现在流水线卡
    expect(screen.getAllByText('1.0.0').length).toBeGreaterThan(0);
    // CHANGELOG 不可用 → 回退渲染所选(stable)通道的 releaseNotes
    expect(screen.getByText('CHANGELOG')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('修复若干 bug')).toBeInTheDocument());
  });

  it('stable 通道为 null 时不回退展示当前版本，按钮显示「暂无可用版本」且禁用', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, stable: null, lastCheckedAt: null });
    await renderPage();
    // stable=null（缓存未填充 / 尚无对应 release）→ 绝不把当前 beta 版当作「最新 稳定」回显；
    // 升级按钮显示「暂无可用版本」且禁用，避免误导（修复「最新 stable 显示成当前 beta」）
    const btn = await screen.findByRole('button', { name: '暂无可用版本' });
    expect(btn).toBeDisabled();
    expect(screen.queryByText(/一键升级到 1\.0\.0/)).toBeNull();
  });

  it('canApply 为 false 时升级按钮禁用（架构不匹配）', async () => {
    mockApi.updatesStatus.mockResolvedValue({
      ...baseStatus,
      stable: { ...baseStatus.stable!, canApply: false },
    });
    await renderPage();
    const btn = await screen.findByRole('button', { name: /一键升级到 1\.1\.0/ });
    expect(btn).toBeDisabled();
  });

  it('点击检查更新发现新版本时弹成功 toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue(baseStatus);
    await renderPage();
    const btn = await screen.findByRole('button', { name: '检查更新' });
    fireEvent.click(btn);
    await waitFor(() => expect(mockApi.updatesCheck).toHaveBeenCalled());
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('点击检查更新且无可用更新时弹「已是最新」info toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue({
      ...baseStatus,
      stable: { ...baseStatus.stable!, hasUpdate: false, canApply: false },
      beta: null,
    });
    await renderPage();
    const btn = await screen.findByRole('button', { name: '检查更新' });
    fireEvent.click(btn);
    await waitFor(() => expect(mockToast.info).toHaveBeenCalled());
  });

  it('检查失败时弹 error toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockRejectedValue(new Error('net'));
    await renderPage();
    const btn = await screen.findByRole('button', { name: '检查更新' });
    fireEvent.click(btn);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('打开确认弹窗并通过「开始升级」发起 apply', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesApply.mockResolvedValue(undefined);
    await renderPage();
    const upgradeBtn = await screen.findByRole('button', { name: /一键升级到 1\.1\.0/ });
    fireEvent.click(upgradeBtn);
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('开始升级'));
    // doApply 以 (channel, targetVersion, currentVersion) 调用
    await waitFor(() =>
      expect(mockApi.updatesApply).toHaveBeenCalledWith('stable', '1.1.0', '1.0.0'),
    );
  });

  it('取消确认弹窗后不发起 apply', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    const upgradeBtn = await screen.findByRole('button', { name: /一键升级到 1\.1\.0/ });
    fireEvent.click(upgradeBtn);
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认一键更新')).not.toBeInTheDocument());
    expect(mockApi.updatesApply).not.toHaveBeenCalled();
  });
});

describe('UpdatesPage — Beta 通道切换与跨通道升级', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('切到 Beta 通道后流水线展示 beta 最新版本号', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, beta: betaChannel });
    await renderPage();
    await waitFor(() => expect(screen.getByText('升级流水线')).toBeInTheDocument());
    await selectBeta();
    await waitFor(() => expect(screen.getAllByText('v0.6.0-beta.3').length).toBeGreaterThan(0));
  });

  it('切到 Beta 通道后升级按钮目标为 beta 版本', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, beta: betaChannel });
    await renderPage();
    await waitFor(() => expect(screen.getByText('升级流水线')).toBeInTheDocument());
    await selectBeta();
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /一键升级到 v0\.6\.0-beta\.3/ })).toBeInTheDocument(),
    );
  });

  it('从 Beta 通道升级时以 channel="beta" 调用 updatesApply', async () => {
    const status: AdminUpdateStatus = {
      ...baseStatus,
      stable: { ...baseStatus.stable!, hasUpdate: false, canApply: false, latestVersion: '1.0.0' },
      beta: betaChannel,
    };
    mockApi.updatesStatus.mockResolvedValue(status);
    mockApi.updatesApply.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('升级流水线')).toBeInTheDocument());
    await selectBeta();
    const upgradeBtn = await screen.findByRole('button', { name: /一键升级到 v0\.6\.0-beta\.3/ });
    fireEvent.click(upgradeBtn);
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('开始升级'));
    await waitFor(() =>
      expect(mockApi.updatesApply).toHaveBeenCalledWith('beta', 'v0.6.0-beta.3', '1.0.0'),
    );
  });

  it('beta 无更新时切到 Beta 升级按钮禁用', async () => {
    mockApi.updatesStatus.mockResolvedValue({
      ...baseStatus,
      beta: { ...betaChannel, hasUpdate: false, canApply: false },
    });
    await renderPage();
    await waitFor(() => expect(screen.getByText('升级流水线')).toBeInTheDocument());
    await selectBeta();
    const btn = await screen.findByRole('button', { name: /一键升级到 v0\.6\.0-beta\.3/ });
    expect(btn).toBeDisabled();
  });
});

describe('UpdatesPage — 维护模式与备份', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('切换维护模式调用 setMaintenance', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.getSettings.mockResolvedValue({ maintenanceMode: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText('维护模式')).toBeInTheDocument());
    const toggle = document.querySelector('.switch input[type="checkbox"]') as HTMLInputElement;
    expect(toggle).toBeTruthy();
    fireEvent.change(toggle, { target: { checked: true } });
    await waitFor(() => expect(mockApi.setMaintenance).toHaveBeenCalledWith(true));
  });

  it('点击「立即备份」调用 updatesCreateBackup 并提示成功', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    const btn = await screen.findByRole('button', { name: '立即备份' });
    fireEvent.click(btn);
    await waitFor(() => expect(mockApi.updatesCreateBackup).toHaveBeenCalled());
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });
});
