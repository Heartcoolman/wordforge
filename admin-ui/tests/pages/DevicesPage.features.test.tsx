import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 重构后的 DevicesPage(设备与遥测)调用以下 adminApi 方法:
// 挂载时:getClients(5s 轮询) / getClientsDistribution / getVersionGate
// 交互:banClient / unbanClient / requestTelemetry
// 遥测详情抽屉:getClientDetail / getTelemetrySummary / getTelemetry
// 升级 / 门控:putUpgradePolicy / setVersionGate / broadcastUpgrade
vi.mock('@/api/admin', () => ({
  adminApi: {
    getClients: vi.fn(),
    getClientsDistribution: vi.fn(),
    getVersionGate: vi.fn(),
    getClientsPaginated: vi.fn(),
    getClientDetail: vi.fn(),
    getTelemetrySummary: vi.fn(),
    getTelemetry: vi.fn(),
    banClient: vi.fn(),
    unbanClient: vi.fn(),
    requestTelemetry: vi.fn(),
    putUpgradePolicy: vi.fn(),
    setVersionGate: vi.fn(),
    broadcastUpgrade: vi.fn(),
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
  appVersion: '1.2.0',
  connectedSecs: 60,
  connectionCount: 1,
  dataChannels: { amas: 'uploaded' as const, learning: 'uploaded' as const, telemetry: 'uploaded' as const },
  isBanned: false,
};
const recentEntry = {
  deviceId: 'dev-recent-12345',
  platform: 'macos',
  userId: 'usr-recent',
  appVersion: '1.1.0',
  lastSeenAt: '2026-04-15 10:00:00',
  isBanned: false,
  dataChannels: { amas: 'nil' as const, learning: 'none' as const, telemetry: 'uploaded' as const },
};
const recentNoUser = {
  deviceId: 'dev-12345', platform: 'web', userId: null, appVersion: null,
  lastSeenAt: '2026-04-15 10:00:00', isBanned: true,
  dataChannels: { amas: 'none' as const, learning: 'none' as const, telemetry: 'none' as const },
};

const emptyProfile = {
  cpuCores: 8, memoryGb: 16, screenWidth: 1920, screenHeight: 1080, pixelRatio: 2,
  osName: 'macOS', browserName: 'Chrome', browserVersion: '120',
  timezone: 'Asia/Shanghai', language: 'zh-CN', touchSupport: false, onlineStatus: true,
};
const telemetryRecord = {
  id: 'tel-1', deviceId: 'dev-1', userId: 'u1',
  eventType: 'session.summary',
  serverTs: '2026-04-15 10:00:00',
  deviceProfile: emptyProfile,
  sessionStats: { sessionDurationSecs: 300, actionsPerMin: 12.5, errorCount: 0, avgResponseTimeMs: 120.4 },
  behaviorSummary: { currentRoute: null, clickCount: null, clickTargets: null, scrollDepthPct: null, visibilityChanges: null, routeChanges: null },
  featureUsage: {},
};

const emptyDistribution = { platforms: [], versions: [], policies: [] };
const emptyGate = { enabled: false, minClientVersion: null, envMinClientVersion: null, effectiveMinClientVersion: null, strictModeEnabled: false };
const emptySummary = {
  total: 0, firstTs: null, lastTs: null, byEventType: [], deviceProfile: null,
  featureUsage: [], routes: [], clickTargets: [], totalClicks: 0, totalErrors: 0, totalDurationSecs: 0, sessionCount: 0,
};

function clientDetail(over: Partial<Record<string, unknown>> = {}) {
  return {
    deviceId: 'dev-1', platform: 'web', userId: 'u1', appVersion: '1.2.0', model: null, country: 'CN',
    firstSeenAt: '2026-04-01 00:00:00', lastSeenAt: '2026-04-15 10:00:00',
    isBanned: false, bannedAt: null, banReason: null, online: true, connectionCount: 1,
    telemetry: { total: 1, latest: null },
    ...over,
  };
}

async function renderPage() {
  const { default: Page } = await import('@/pages/DevicesPage');
  return renderWithProviders(() => <Page />);
}

describe('DevicesPage — tabs, ban dialog, telemetry drawer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // 挂载时三资源默认安全返回值,避免 unhandled rejection
    mockApi.getClientsDistribution.mockResolvedValue(emptyDistribution);
    mockApi.getVersionGate.mockResolvedValue(emptyGate);
    mockApi.getClientsPaginated.mockResolvedValue({ data: [], total: 0, page: 1, perPage: 14, totalPages: 1 });
    mockApi.getClientDetail.mockResolvedValue(clientDetail());
    mockApi.getTelemetrySummary.mockResolvedValue(emptySummary);
    mockApi.getTelemetry.mockResolvedValue({ records: [], total: 0 });
  });

  it('clicking 实时连接 tab switches back from 近期活跃', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [recentEntry] });
    await renderPage();
    // 页面头部 + 默认 SSE tab 行(web 平台徽章)
    await waitFor(() => expect(screen.getByText('设备与遥测')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('web')).toBeInTheDocument());
    // 切到近期活跃 → macos 平台徽章出现
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
    // 切回实时连接 → web 重新出现
    fireEvent.click(screen.getByText('实时连接'));
    await waitFor(() => expect(screen.getByText('web')).toBeInTheDocument());
  });

  it('ban + telemetry-detail buttons in 近期活跃 tab', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentEntry] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
    // 封禁 → 确认 Modal,随后取消
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('封禁设备')).not.toBeInTheDocument());
    // 遥测详情 → 抽屉打开,getTelemetry 被调用
    fireEvent.click(screen.getByText('遥测详情'));
    await waitFor(() => expect(screen.getByText('设备遥测详情')).toBeInTheDocument());
    await waitFor(() => expect(mockApi.getTelemetry).toHaveBeenCalled());
  });

  it('ban dialog cancel button closes it', async () => {
    // 封禁确认走通用 Modal,取消按钮是稳定 UX 入口;backdrop 关闭由 Modal 自身测试覆盖。
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('封禁设备')).not.toBeInTheDocument());
  });

  it('confirms ban → calls adminApi.banClient', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.banClient.mockResolvedValue({ banned: true, deviceId: sseEntry.deviceId });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockApi.banClient).toHaveBeenCalledWith(sseEntry.deviceId, undefined));
  });

  it('recent entry with null userId renders em-dash placeholder', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentNoUser] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    // isBanned=true → 操作列出现"解封"按钮
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    // userId=null / appVersion=null → shortId 渲染 em-dash "—"(多处,故用 getAllByText)
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
  });

  it('telemetry drawer loads summary + records for a device', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetrySummary.mockResolvedValue({
      ...emptySummary,
      total: 1, firstTs: '2026-04-15 10:00:00', lastTs: '2026-04-15 10:00:00',
      byEventType: [{ eventType: 'session.summary', count: 1, avgDurationSecs: 300, totalErrors: 0, avgActionsPerMin: 12.5, avgResponseMs: 120 }],
      deviceProfile: emptyProfile, sessionCount: 1, totalDurationSecs: 300,
    });
    mockApi.getTelemetry.mockResolvedValue({ records: [telemetryRecord], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('遥测详情')).toBeInTheDocument());
    fireEvent.click(screen.getByText('遥测详情'));
    // 抽屉标题 + 遥测总览区块
    await waitFor(() => expect(screen.getByText('设备遥测详情')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText(/遥测总览/)).toBeInTheDocument());
    // 遥测记录行渲染(eventType 文本)
    await waitFor(() => expect(screen.getAllByText('session.summary').length).toBeGreaterThan(0));
    expect(mockApi.getClientDetail).toHaveBeenCalled();
    expect(mockApi.getTelemetrySummary).toHaveBeenCalled();
  });
});
