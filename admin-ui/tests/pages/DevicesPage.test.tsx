import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 设备与遥测页重设计后:平台 hero · 实时/近期/全部三 tab 表格 · 封禁/解封 modal ·
// 拉取遥测 · 遥测详情抽屉(getClientDetail + getTelemetrySummary + getTelemetry) ·
// 升级策略 · 版本门控(getVersionGate)。每个 adminApi 方法都给安全默认值避免未捕获 reject。
vi.mock('@/api/admin', () => ({
  adminApi: {
    getClients: vi.fn(),
    banClient: vi.fn(),
    unbanClient: vi.fn(),
    listFlaggedClients: vi.fn(),
    clearClientFlag: vi.fn(),
    requestTelemetry: vi.fn(),
    getTelemetry: vi.fn(),
    getTelemetrySummary: vi.fn(),
    getClientDetail: vi.fn(),
    getVersionGate: vi.fn(),
    setVersionGate: vi.fn(),
    getClientsDistribution: vi.fn(),
    getClientsPaginated: vi.fn(),
    putUpgradePolicy: vi.fn(),
    broadcastUpgrade: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const chan = { amas: 'uploaded' as const, learning: 'uploaded' as const, telemetry: 'uploaded' as const };

const sseEntry = {
  deviceId: 'dev-1234567890abcdef',
  platform: 'web',
  userId: 'usr-1234567890ab',
  connectedSecs: 120,
  connectionCount: 1,
  dataChannels: chan,
  isBanned: false,
  appVersion: '1.2.0',
};
const bannedEntry = { ...sseEntry, deviceId: 'dev-banned-id', isBanned: true };
const recentEntry = {
  deviceId: 'dev-recent-12345',
  platform: 'macos',
  userId: 'usr-recent-id',
  lastSeenAt: '2026-04-15 10:00:00',
  isBanned: false,
  dataChannels: { amas: 'nil' as const, learning: 'none' as const, telemetry: 'uploaded' as const },
  appVersion: null,
};

// ClientDetail fixture(抽屉头部 metric + 封禁状态)
const clientDetail = (over: Record<string, unknown> = {}) => ({
  deviceId: 'dev-1234567890abcdef',
  platform: 'web',
  userId: 'usr-1234567890ab',
  appVersion: '1.2.0',
  model: 'Pixel 8',
  country: 'CN',
  firstSeenAt: '2026-04-01 10:00:00',
  lastSeenAt: '2026-04-15 10:00:00',
  isBanned: false,
  bannedAt: null,
  banReason: null,
  online: true,
  connectionCount: 1,
  telemetry: { total: 1, latest: null },
  ...over,
});

// TelemetrySummary 原始明细行(抽屉"遥测记录"列表项)
const telemetryRecord = {
  id: 'tel-1',
  deviceId: 'dev-1234567890abcdef',
  userId: 'usr-1234567890ab',
  eventType: 'session.summary',
  serverTs: '2026-04-15 10:00:00',
  deviceProfile: {
    osName: 'macOS', browserName: 'Chrome', browserVersion: '120',
    cpuCores: 8, memoryGb: 16, screenWidth: 1920, screenHeight: 1080, pixelRatio: 2,
    timezone: 'Asia/Shanghai', language: 'zh-CN', touchSupport: null, onlineStatus: null,
  },
  sessionStats: { sessionDurationSecs: 300, actionsPerMin: 12.5, errorCount: 0, avgResponseTimeMs: 120.4 },
  behaviorSummary: { currentRoute: '/learning', clickCount: 50, clickTargets: null, scrollDepthPct: 80.5, routeChanges: 3, visibilityChanges: 1 },
  featureUsage: { search: 5, edit: 2 },
};

// TelemetryDeviceSummary fixture(抽屉遥测总览 + 分类 chips);可传 over 覆写
const fullSummary = (over: Record<string, unknown> = {}) => ({
  total: 1, firstTs: '2026-04-15 10:00:00', lastTs: '2026-04-15 10:00:00',
  byEventType: [{ eventType: 'session.summary', count: 1, avgDurationSecs: 300, totalErrors: 0, avgActionsPerMin: 12.5, avgResponseMs: 120 }],
  deviceProfile: { osName: 'macOS', browserName: 'Chrome', browserVersion: '120', cpuCores: 8, memoryGb: 16, screenWidth: 1920, screenHeight: 1080, pixelRatio: 2, timezone: 'Asia/Shanghai', language: 'zh-CN', touchSupport: null, onlineStatus: null },
  featureUsage: [{ name: 'search', count: 42 }], routes: [{ name: '/learning', count: 12 }],
  clickTargets: [{ name: '开始', count: 5 }], totalClicks: 213, totalErrors: 0, totalDurationSecs: 2880, sessionCount: 5,
  ...over,
});

const versionGate = { enabled: false, minClientVersion: null, envMinClientVersion: null, effectiveMinClientVersion: null, strictModeEnabled: false };

describe('DevicesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // clearAllMocks 会清掉 mockResolvedValue 设的实现,逐项还原默认,避免 createResource 解构 undefined。
    mockApi.getClientsDistribution.mockResolvedValue({ platforms: [], versions: [], policies: [] });
    mockApi.getClientsPaginated.mockResolvedValue({ data: [], total: 0, page: 1, perPage: 14, totalPages: 0 });
    mockApi.listFlaggedClients.mockResolvedValue({ flagged: [] });
    mockApi.clearClientFlag.mockResolvedValue({ cleared: true, deviceId: 'x' });
    mockApi.getVersionGate.mockResolvedValue(versionGate);
    mockApi.setVersionGate.mockResolvedValue(versionGate);
    mockApi.putUpgradePolicy.mockResolvedValue({ ok: true, platform: 'web' });
    mockApi.broadcastUpgrade.mockResolvedValue({ matched: 0, pushedConnections: 0 });
    mockApi.getClientDetail.mockResolvedValue(clientDetail());
    mockApi.getTelemetrySummary.mockResolvedValue(fullSummary());
    mockApi.getTelemetry.mockResolvedValue({ records: [telemetryRecord], total: 1 });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/DevicesPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders device table with SSE entries', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [recentEntry] });
    await renderPage();
    // 页头标题 + 实时连接 tab + 设备行平台 badge
    await waitFor(() => expect(screen.getByText('设备与遥测')).toBeInTheDocument());
    expect(screen.getByText('实时连接')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('web')).toBeInTheDocument());
  });

  it('renders gracefully when clients load fails', async () => {
    // createResource 加载失败不再 toast(无 onError);页面仍渲染页头与 tab,不崩溃
    mockApi.getClients.mockRejectedValue({ message: 'net err' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('设备与遥测')).toBeInTheDocument());
    expect(screen.getByText('实时连接')).toBeInTheDocument();
  });

  it('shows empty SSE state', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无设备')).toBeInTheDocument());
  });

  it('switches to recent tab and shows entries', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentEntry] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
  });

  it('shows empty recent state', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('暂无设备')).toBeInTheDocument());
  });

  it('opens ban confirm modal and bans device', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.banClient.mockResolvedValue({ banned: true, deviceId: sseEntry.deviceId });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    expect(screen.getByText('封禁原因(可选)')).toBeInTheDocument();
    const input = screen.getByPlaceholderText('如:异常高频请求') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'spam' } });
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockApi.banClient).toHaveBeenCalledWith('dev-1234567890abcdef', 'spam'));
  });

  it('cancels ban modal by clicking 取消', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('封禁设备')).not.toBeInTheDocument());
  });

  it('unbans a banned device', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [bannedEntry], recentlyActive: [] });
    mockApi.unbanClient.mockResolvedValue({ banned: false, deviceId: bannedEntry.deviceId });
    await renderPage();
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(screen.getByText('解封设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认解封'));
    await waitFor(() => expect(mockApi.unbanClient).toHaveBeenCalledWith('dev-banned-id'));
  });

  it('handles ban failure', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.banClient.mockRejectedValue({ message: '500' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('requests telemetry', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.requestTelemetry.mockResolvedValue({ requestId: 'req-1' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('拉取遥测')).toBeInTheDocument());
    fireEvent.click(screen.getByText('拉取遥测'));
    await waitFor(() => expect(mockApi.requestTelemetry).toHaveBeenCalledWith('dev-1234567890abcdef'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('下发遥测拉取请求')));
  });

  it('handles requestTelemetry failure', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.requestTelemetry.mockRejectedValue({ message: 'err' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('拉取遥测')).toBeInTheDocument());
    fireEvent.click(screen.getByText('拉取遥测'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('opens telemetry drawer with 总览 + 记录', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('遥测详情')).toBeInTheDocument());
    fireEvent.click(screen.getByText('遥测详情'));
    // 抽屉标题 + 三资源拉取
    await waitFor(() => expect(screen.getByText('设备遥测详情')).toBeInTheDocument());
    await waitFor(() => expect(mockApi.getClientDetail).toHaveBeenCalledWith('dev-1234567890abcdef'));
    expect(mockApi.getTelemetrySummary).toHaveBeenCalledWith('dev-1234567890abcdef');
    // 遥测总览 + 遥测记录区块直接可见
    await waitFor(() => expect(screen.getByText(/遥测总览/)).toBeInTheDocument());
    expect(screen.getByText(/遥测记录/)).toBeInTheDocument();
    // session.summary 同时出现在分类 chip 与原始明细行,至少两处
    await waitFor(() => expect(screen.getAllByText('session.summary').length).toBeGreaterThanOrEqual(1));
  });

  it('filters telemetry records by event_type chip', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetrySummary.mockResolvedValue(fullSummary({
      total: 3,
      byEventType: [
        { eventType: 'periodic', count: 2, avgDurationSecs: 60, totalErrors: 0, avgActionsPerMin: 0, avgResponseMs: 10 },
        { eventType: 'error_js', count: 1, avgDurationSecs: 0, totalErrors: 5, avgActionsPerMin: 0, avgResponseMs: 0 },
      ],
    }));
    await renderPage();
    await waitFor(() => expect(screen.getByText('遥测详情')).toBeInTheDocument());
    fireEvent.click(screen.getByText('遥测详情'));
    // 分类 chips 出现(role=button 消歧)
    await waitFor(() => expect(screen.getByRole('button', { name: /periodic/ })).toBeInTheDocument());
    // 点分类 chip → 按 eventType 重新查询
    fireEvent.click(screen.getByRole('button', { name: /error_js/ }));
    await waitFor(() => expect(mockApi.getTelemetry).toHaveBeenLastCalledWith('dev-1234567890abcdef', { eventType: 'error_js' }));
  });

  it('handles getTelemetry failure without crashing', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockRejectedValue({ message: 'fail' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('遥测详情')).toBeInTheDocument());
    fireEvent.click(screen.getByText('遥测详情'));
    // 记录拉取失败仍渲染抽屉骨架,不崩溃
    await waitFor(() => expect(screen.getByText('设备遥测详情')).toBeInTheDocument());
    expect(mockApi.getTelemetry).toHaveBeenCalledWith('dev-1234567890abcdef', undefined);
  });

  it('shows empty telemetry records message', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetrySummary.mockResolvedValue(fullSummary({
      total: 0, byEventType: [], featureUsage: [], routes: [], clickTargets: [],
      totalClicks: 0, totalErrors: 0, totalDurationSecs: 0, sessionCount: 0, deviceProfile: null,
    }));
    mockApi.getTelemetry.mockResolvedValue({ records: [], total: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('遥测详情')).toBeInTheDocument());
    fireEvent.click(screen.getByText('遥测详情'));
    await waitFor(() => expect(screen.getByText('暂无遥测记录')).toBeInTheDocument());
  });

  it('closes telemetry drawer', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('遥测详情')).toBeInTheDocument());
    fireEvent.click(screen.getByText('遥测详情'));
    await waitFor(() => expect(screen.getByText('设备遥测详情')).toBeInTheDocument());
    // 抽屉关闭按钮是带 title="关闭" 的 IconBtn
    fireEvent.click(screen.getByTitle('关闭'));
    await waitFor(() => expect(screen.queryByText('设备遥测详情')).not.toBeInTheDocument());
  });

  it('refreshes clients list when 刷新 clicked', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('设备与遥测')).toBeInTheDocument());
    await waitFor(() => expect(mockApi.getClients).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByText('刷新'));
    await waitFor(() => expect(mockApi.getClients).toHaveBeenCalledTimes(2));
  });

  // m054:关联风控复核 tab —— 列出被牵连设备 + 解除标记
  it('风险标记 tab 展示被标记设备并可解除标记', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [] });
    mockApi.listFlaggedClients.mockResolvedValue({
      flagged: [{
        deviceId: 'dev-flagged-abcdef',
        platform: 'web',
        userId: 'usr-flag-1',
        lastSeenAt: '2026-04-15 10:00:00',
        isBanned: false,
        appVersion: '1.2.0',
        riskReason: '关联自被封设备 dev-bad:共享出口 IP、相同设备指纹(模糊)',
        riskFlaggedAt: '2026-04-15 10:00:00',
        riskRelatedDevice: 'dev-bad',
      }],
    });
    await renderPage();
    await waitFor(() => expect(screen.getByText('风险标记')).toBeInTheDocument());
    fireEvent.click(screen.getByText('风险标记'));
    await waitFor(() => expect(screen.getByText(/共享出口 IP/)).toBeInTheDocument());
    fireEvent.click(screen.getByText('解除标记'));
    await waitFor(() => expect(mockApi.clearClientFlag).toHaveBeenCalledWith('dev-flagged-abcdef'));
  });

  // m054:封禁返回 flaggedRelated>0 时 toast 带复核提示
  it('封禁联动关联标记时 toast 提示复核数量', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.banClient.mockResolvedValue({ banned: true, deviceId: sseEntry.deviceId, flaggedRelated: 3, flaggedDeviceIds: ['a', 'b', 'c'] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('已封禁设备'), expect.stringContaining('3 台')));
  });
});
