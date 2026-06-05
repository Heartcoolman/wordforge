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
    getTelemetrySummary: vi.fn(),
    // m027:DevicesPage 现在并行拉 distribution(失败不挡 SSE 渲染),默认 mock 返空聚合
    getClientsDistribution: vi.fn(() => Promise.resolve({ platforms: [], versions: [], policies: [] })),
    getClientsPaginated: vi.fn(() => Promise.resolve({ data: [], total: 0, page: 1, perPage: 20, totalPages: 0 })),
    listUpgradePolicy: vi.fn(() => Promise.resolve({ policies: [] })),
    putUpgradePolicy: vi.fn(() => Promise.resolve({ ok: true, platform: 'web' })),
    broadcastUpgrade: vi.fn(() => Promise.resolve({ matched: 0, pushedConnections: 0 })),
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

// 设备遥测分类总览 + 操作概览 fixture(覆盖 digest 所有字段);可传 over 覆写
const fullSummary = (over: Record<string, unknown> = {}) => ({
  total: 1, firstTs: '2026-04-15 10:00:00', lastTs: '2026-04-15 10:00:00',
  byEventType: [{ eventType: 'session.summary', count: 1, avgDurationSecs: 300, totalErrors: 0, avgActionsPerMin: 12.5, avgResponseMs: 120 }],
  deviceProfile: { osName: 'macOS', browserName: 'Chrome', browserVersion: '120', cpuCores: 8, memoryGb: 16, screenWidth: 1920, screenHeight: 1080, pixelRatio: 2, timezone: 'Asia/Shanghai', language: 'zh-CN', touchSupport: null, onlineStatus: null },
  featureUsage: [{ name: 'search', count: 42 }], routes: [{ name: '/learning', count: 12 }],
  clickTargets: [{ name: '开始', count: 5 }], totalClicks: 213, totalErrors: 0, totalDurationSecs: 2880, sessionCount: 5,
  ...over,
});

describe('DevicesPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // m027:vi.clearAllMocks 会清掉 mockResolvedValue 设的实现,需要在每次 it 前还原默认值,
    // 否则 loadDevices 内 Promise.all 第二项(getClientsDistribution)返 undefined 导致解构失败。
    mockApi.getClientsDistribution.mockResolvedValue({ platforms: [], versions: [], policies: [] });
    mockApi.getClientsPaginated.mockResolvedValue({ data: [], total: 0, page: 1, perPage: 20, totalPages: 0 });
    mockApi.listUpgradePolicy.mockResolvedValue({ policies: [] });
    mockApi.putUpgradePolicy.mockResolvedValue({ ok: true, platform: 'web' });
    mockApi.broadcastUpgrade.mockResolvedValue({ matched: 0, pushedConnections: 0 });
    // 遥测面板打开时并行拉分类总览 + 操作概览;默认给一类 + 设备画像 + 聚合,保证面板可渲染
    mockApi.getTelemetrySummary.mockResolvedValue(fullSummary());
  });

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

  it('loads telemetry 概览 + 设备画像,原始明细默认折叠', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockResolvedValue({ records: [telemetryRecord], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText(/遥测记录/)).toBeInTheDocument());
    // 操作概览 + 设备画像默认直接可见(无需翻记录)
    await waitFor(() => expect(screen.getByText('这台设备做了什么')).toBeInTheDocument());
    expect(screen.getByText('设备画像')).toBeInTheDocument();
    // 展开原始明细 → session.summary 出现次数增加(新增 chip + 行徽章)
    const before = screen.getAllByText('session.summary').length;
    fireEvent.click(screen.getByText(/原始明细/));
    await waitFor(() => expect(screen.getAllByText('session.summary').length).toBeGreaterThan(before));
  });

  it('telemetry 明细按 event_type 分类 + 行点开展开', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [sseEntry], recentlyActive: [] });
    mockApi.getTelemetrySummary.mockResolvedValue(fullSummary({
      total: 3,
      byEventType: [
        { eventType: 'periodic', count: 2, avgDurationSecs: 60, totalErrors: 0, avgActionsPerMin: 0, avgResponseMs: 10 },
        { eventType: 'error_js', count: 1, avgDurationSecs: 0, totalErrors: 5, avgActionsPerMin: 0, avgResponseMs: 0 },
      ],
    }));
    mockApi.getTelemetry.mockResolvedValue({ records: [telemetryRecord], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText('这台设备做了什么')).toBeInTheDocument());
    // 明细默认折叠:会话统计不可见
    expect(screen.queryByText('会话统计')).not.toBeInTheDocument();
    // 展开原始明细 → 分类 chips 出现(用 role=button 消歧,避免与概览异常行同名 event 冲突)
    fireEvent.click(screen.getByText(/原始明细/));
    await waitFor(() => expect(screen.getByRole('button', { name: /periodic/ })).toBeInTheDocument());
    // 点分类 chip → 按 eventType 重新查询(全量计数走 summary,不受分页影响)
    fireEvent.click(screen.getByRole('button', { name: /error_js/ }));
    await waitFor(() => expect(mockApi.getTelemetry).toHaveBeenLastCalledWith('dev-1234567890abcdef', { eventType: 'error_js' }));
    // 点记录行 → 展开看完整会话统计
    fireEvent.click(screen.getByText('session.summary'));
    await waitFor(() => expect(screen.getByText('会话统计')).toBeInTheDocument());
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
    // 设备无任何遥测:概览返空总览 → 顶部直接显示空态
    mockApi.getTelemetrySummary.mockResolvedValue(fullSummary({
      total: 0, byEventType: [], featureUsage: [], routes: [], clickTargets: [],
      totalClicks: 0, totalErrors: 0, totalDurationSecs: 0, sessionCount: 0, deviceProfile: null,
    }));
    mockApi.getTelemetry.mockResolvedValue({ records: [], total: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText('暂无遥测数据')).toBeInTheDocument());
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
