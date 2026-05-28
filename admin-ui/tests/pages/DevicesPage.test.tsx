import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getClients: vi.fn(),
    banClient: vi.fn(),
    unbanClient: vi.fn(),
    requestTelemetry: vi.fn(),
    getTelemetry: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const sseEntry = {
  deviceId: 'dev-1234567890abcdef',
  platform: 'web',
  userId: 'usr-1234567890ab',
  connectedSecs: 120,
  connectionCount: 1,
  dataChannels: { amas: 'uploaded' as const, learning: 'uploaded' as const, telemetry: 'uploaded' as const },
  isBanned: false,
};
const bannedEntry = { ...sseEntry, deviceId: 'dev-banned-id', isBanned: true };
const recentEntry = {
  deviceId: 'dev-recent-12345',
  platform: 'macos',
  userId: 'usr-recent-id',
  lastSeenAt: '2026-04-15 10:00:00',
  isBanned: false,
  dataChannels: { amas: 'nil' as const, learning: 'none' as const, telemetry: 'uploaded' as const },
};

const telemetryRecord = {
  eventType: 'session.summary',
  serverTs: '2026-04-15 10:00:00',
  deviceProfile: {
    osName: 'macOS', browserName: 'Chrome', browserVersion: '120',
    cpuCores: 8, memoryGb: 16, screenWidth: 1920, screenHeight: 1080, pixelRatio: 2,
    timezone: 'Asia/Shanghai', language: 'zh-CN',
  },
  sessionStats: { sessionDurationSecs: 300, actionsPerMin: 12.5, errorCount: 0, avgResponseTimeMs: 120.4 },
  behaviorSummary: { currentRoute: '/learning', clickCount: 50, scrollDepthPct: 80.5, routeChanges: 3, visibilityChanges: 1 },
  featureUsage: { search: 5, edit: 2 },
};

describe('DevicesPage', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/DevicesPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders SSE table with entries', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [recentEntry] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/SSE 实时连接/)).toBeInTheDocument());
    expect(screen.getByText('web')).toBeInTheDocument();
  });

  it('shows load failure toast', async () => {
    mockApi.getClients.mockRejectedValue({ message: 'net err' });
    await renderPage();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('shows empty SSE state', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无活跃 SSE 连接')).toBeInTheDocument());
  });

  it('switches to recent tab and shows entries', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentEntry] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
  });

  it('shows empty recent state', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('暂无近期活跃设备')).toBeInTheDocument());
  });

  it('opens ban confirm dialog and bans device', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.banClient.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('封禁原因（可选）') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'spam' } });
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockApi.banClient).toHaveBeenCalledWith('dev-1234567890abcdef', 'spam'));
  });

  it('cancels ban dialog by clicking 取消', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认封禁设备')).not.toBeInTheDocument());
  });

  it('unbans a banned device', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [bannedEntry], recentlyActive: [] });
    mockApi.unbanClient.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(screen.getByText('确认解封设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认解封'));
    await waitFor(() => expect(mockApi.unbanClient).toHaveBeenCalled());
  });

  it('handles ban failure', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.banClient.mockRejectedValue({ message: '500' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('requests telemetry', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.requestTelemetry.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('拉取遥测')).toBeInTheDocument());
    fireEvent.click(screen.getByText('拉取遥测'));
    // toast 文案现在带 device id 前缀
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('发送遥测请求')));
  });

  it('handles requestTelemetry failure', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.requestTelemetry.mockRejectedValue({ message: 'err' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('拉取遥测')).toBeInTheDocument());
    fireEvent.click(screen.getByText('拉取遥测'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('loads telemetry history and renders sections', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockResolvedValue({ records: [telemetryRecord], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText(/遥测记录/)).toBeInTheDocument());
    expect(screen.getByText('session.summary')).toBeInTheDocument();
    expect(screen.getByText('设备信息')).toBeInTheDocument();
  });

  it('handles getTelemetry failure', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockRejectedValue({ message: 'fail' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('shows empty telemetry message', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockResolvedValue({ records: [], total: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText('暂无遥测记录')).toBeInTheDocument());
  });

  it('closes telemetry panel', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockResolvedValue({ records: [telemetryRecord], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText('关闭')).toBeInTheDocument());
    fireEvent.click(screen.getByText('关闭'));
    await waitFor(() => expect(screen.queryByText('关闭')).not.toBeInTheDocument());
  });

  it('refreshes clients list when 刷新 clicked', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/SSE 实时连接/)).toBeInTheDocument());
    fireEvent.click(screen.getByText('刷新'));
    await waitFor(() => expect(mockApi.getClients).toHaveBeenCalledTimes(2));
  });
});
