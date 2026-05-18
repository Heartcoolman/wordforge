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
  ApiError: class ApiError extends Error { status = 0; code = ''; constructor(m: string, status: number, code: string) { super(m); this.status = status; this.code = code; } },
  connectSseStream: vi.fn(() => () => undefined),
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const baseStatus = {
  currentVersion: '1.0.0',
  latestVersion: '1.1.0',
  hasUpdate: true,
  canApply: true,
  autoCheckEnabled: true,
  lastCheckedAt: '2026-04-15T10:00:00Z',
  releaseNotes: '修复若干 bug',
  releaseUrl: 'https://github.com/x/y/releases/v1.1.0',
};

describe('UpdatesPage extra', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/admin/UpdatesPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders status cards and release notes', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByText('当前版本')).toBeInTheDocument());
    expect(screen.getByText('1.0.0')).toBeInTheDocument();
    expect(screen.getByText('Release Notes')).toBeInTheDocument();
    expect(screen.getByText('修复若干 bug')).toBeInTheDocument();
  });

  it('shows nil values when latestVersion absent', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, latestVersion: null, lastCheckedAt: null, hasUpdate: false, releaseNotes: null });
    await renderPage();
    await waitFor(() => expect(screen.getByText('尚未检查')).toBeInTheDocument());
    expect(screen.getByText('从未检查')).toBeInTheDocument();
  });

  it('shows arch warning when hasUpdate but canApply is false', async () => {
    mockApi.updatesStatus.mockResolvedValue({ ...baseStatus, canApply: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/未找到匹配当前架构的产物/)).toBeInTheDocument());
  });

  it('triggers check and shows update found toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue({ ...baseStatus, hasUpdate: true });
    await renderPage();
    await waitFor(() => expect(screen.getByText('立即检查')).toBeInTheDocument());
    fireEvent.click(screen.getByText('立即检查'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
  });

  it('triggers check and shows up-to-date toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockResolvedValue({ ...baseStatus, hasUpdate: false });
    await renderPage();
    await waitFor(() => expect(screen.getByText('立即检查')).toBeInTheDocument());
    fireEvent.click(screen.getByText('立即检查'));
    await waitFor(() => expect(mockToast.info).toHaveBeenCalled());
  });

  it('shows check failure toast', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesCheck.mockRejectedValue(new Error('net'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('立即检查')).toBeInTheDocument());
    fireEvent.click(screen.getByText('立即检查'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('opens confirm dialog and applies via 开始升级', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    mockApi.updatesApply.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/一键更新到/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/一键更新到/));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('开始升级'));
    await waitFor(() => expect(mockApi.updatesApply).toHaveBeenCalled());
  });

  it('cancels apply confirm dialog', async () => {
    mockApi.updatesStatus.mockResolvedValue(baseStatus);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/一键更新到/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/一键更新到/));
    await waitFor(() => expect(screen.getByText('确认一键更新')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
  });
});
