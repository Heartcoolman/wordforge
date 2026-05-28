import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

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
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const sseEntry = {
  deviceId: 'dev-1234567890abcdef',
  platform: 'web',
  userId: 'usr-12345',
  connectedSecs: 60,
  connectionCount: 1,
  dataChannels: { amas: 'uploaded' as const, learning: 'uploaded' as const, telemetry: 'uploaded' as const },
  isBanned: false,
};
const recentEntry = {
  deviceId: 'dev-recent-12345',
  platform: 'macos',
  userId: 'usr-recent',
  lastSeenAt: '2026-04-15 10:00:00',
  isBanned: false,
  dataChannels: { amas: 'nil' as const, learning: 'none' as const, telemetry: 'uploaded' as const },
};
const recentNoUser = {
  deviceId: 'dev-12345', platform: 'web', userId: null,
  lastSeenAt: '2026-04-15 10:00:00', isBanned: true,
  dataChannels: { amas: 'none' as const, learning: 'none' as const, telemetry: 'none' as const },
};
const telemetryNullBehavior = {
  eventType: 'session.summary',
  serverTs: '2026-04-15 10:00:00',
  deviceProfile: { osName: 'macOS', browserName: 'Chrome', browserVersion: '120', cpuCores: 8, memoryGb: 16, screenWidth: 1920, screenHeight: 1080, pixelRatio: 2, timezone: 'Asia/Shanghai', language: 'zh-CN' },
  sessionStats: { sessionDurationSecs: 300, actionsPerMin: 12.5, errorCount: 0, avgResponseTimeMs: 120.4 },
  behaviorSummary: { currentRoute: null, clickCount: null, scrollDepthPct: null, routeChanges: null, visibilityChanges: null },
  featureUsage: {},
};

async function renderPage() {
  const { default: Page } = await import('@/pages/admin/ClientsPage');
  return renderWithProviders(() => <Page />);
}

describe('ClientsPage — tabs, ban dialog, telemetry edge data', () => {
  beforeEach(() => vi.clearAllMocks());

  it('clicking SSE tab button switches back from recent', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [recentEntry] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
    fireEvent.click(screen.getByText(/SSE 实时连接/));
    await waitFor(() => expect(screen.getByText('web')).toBeInTheDocument());
  });

  it('clicks 封禁 / 历史 buttons in recent tab', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentEntry] });
    mockApi.getTelemetry.mockResolvedValue({ records: [], total: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(mockApi.getTelemetry).toHaveBeenCalled());
  });

  it('ban dialog cancel button closes it', async () => {
    // ConfirmDialog 重构后用通用 Modal，cancel 按钮是稳定 UX 入口；
    // backdrop click 关闭由 Modal 自己测试覆盖，不在此重复。
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认封禁设备')).not.toBeInTheDocument());
  });

  it('recent entry with null userId renders "-"', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentNoUser] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('已封禁')).toBeInTheDocument());
    expect(screen.getByRole('cell', { name: '-' })).toBeInTheDocument();
  });

  it('telemetry with all-null behaviorSummary + empty featureUsage hides those sections', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [{
      deviceId: 'dev-1', platform: 'web', userId: 'u1', connectedSecs: 60, connectionCount: 1,
      dataChannels: { amas: 'uploaded' as const, learning: 'uploaded' as const, telemetry: 'uploaded' as const },
      isBanned: false,
    }], recentlyActive: [] });
    mockApi.getTelemetry.mockResolvedValue({ records: [telemetryNullBehavior], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText('session.summary')).toBeInTheDocument());
    expect(screen.queryByText('行为摘要')).not.toBeInTheDocument();
  });
});
