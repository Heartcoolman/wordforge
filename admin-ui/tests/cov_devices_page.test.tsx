import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from './helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    getClients: vi.fn(),
    getClientsDistribution: vi.fn(),
    getVersionGate: vi.fn(),
    getClientsPaginated: vi.fn(),
    banClient: vi.fn(),
    unbanClient: vi.fn(),
    listFlaggedClients: vi.fn(),
    clearClientFlag: vi.fn(),
    requestTelemetry: vi.fn(),
    getTelemetry: vi.fn(),
    getTelemetrySummary: vi.fn(),
    getClientDetail: vi.fn(),
    putUpgradePolicy: vi.fn(),
    broadcastUpgrade: vi.fn(),
    setVersionGate: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

// 短 ID（<=14 字符）走 shortId 的非截断分支
const shortIdEntry = {
  deviceId: 'dev-short',
  platform: 'android',
  userId: 'u-short',
  connectedSecs: 600, // 600s → 10m
  connectionCount: 3,
  dataChannels: { amas: 'uploaded' as const, learning: 'nil' as const, telemetry: 'none' as const },
  isBanned: false,
};

// recent 列表条目：userId 为 null（走 shortId 的 '—' 分支）、isBanned=true（操作列展示"解封"）
const recentBannedNoUser = {
  deviceId: 'dev-recent-banned-1234567890',
  platform: 'ios',
  userId: null,
  lastSeenAt: '2026-04-15T10:00:00Z',
  isBanned: true,
  dataChannels: { amas: 'none' as const, learning: 'uploaded' as const, telemetry: 'nil' as const },
};

// recent 列表条目：正常状态、带 userId
const recentNormal = {
  deviceId: 'dev-recent-normal-abcdef',
  platform: 'macos',
  userId: 'usr-recent-normal',
  lastSeenAt: '2026-04-15T10:00:00Z',
  isBanned: false,
  dataChannels: { amas: 'uploaded' as const, learning: 'uploaded' as const, telemetry: 'uploaded' as const },
};

// distribution / version-gate 默认空形状，避免页面初始化 createResource 未处理 reject
const emptyDist = { platforms: [], versions: [], policies: [] };
const defaultGate = {
  enabled: false,
  minClientVersion: null,
  envMinClientVersion: null,
  effectiveMinClientVersion: null,
  strictModeEnabled: false,
};

describe('DevicesPage 覆盖补充', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // 所有页面初始化会调用的接口都给安全默认值
    mockApi.getClientsDistribution.mockResolvedValue(emptyDist);
    mockApi.getVersionGate.mockResolvedValue(defaultGate);
    mockApi.getClientsPaginated.mockResolvedValue({ data: [], total: 0, page: 1, perPage: 14, totalPages: 1 });
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [] });
    mockApi.listFlaggedClients.mockResolvedValue({ flagged: [] });
    mockApi.clearClientFlag.mockResolvedValue({ cleared: true, deviceId: 'x' });
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/DevicesPage');
    return renderWithProviders(() => <Page />);
  }

  it('SSE 列表短 ID 原样显示且渲染连接时长/连接数', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    await renderPage();
    // 页面标题
    await waitFor(() => expect(screen.getByText('设备与遥测')).toBeInTheDocument());
    // 短 ID 未截断
    await waitFor(() => expect(screen.getByText('dev-short')).toBeInTheDocument());
    expect(screen.getByText('u-short')).toBeInTheDocument();
    expect(screen.getByText('android')).toBeInTheDocument();
    // 600s = 10m
    expect(screen.getByText('10m')).toBeInTheDocument();
    // 连接数 3
    expect(screen.getByText('3')).toBeInTheDocument();
  });

  it('Tab 标签显示各自计数', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [recentNormal] });
    await renderPage();
    // 三个 tab 标签
    await waitFor(() => expect(screen.getByText('实时连接')).toBeInTheDocument());
    expect(screen.getByText('近期活跃')).toBeInTheDocument();
    expect(screen.getByText('全部设备')).toBeInTheDocument();
    // 实时连接 tab 上挂着计数 1（label 同 span 分离，定位计数 span）
    const sseTab = screen.getByText('实时连接').closest('button')!;
    expect(sseTab.textContent).toMatch(/1/);
  });

  it('recent 列表渲染封禁设备的解封按钮、userId 为 null 显示 —', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentBannedNoUser] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('ios')).toBeInTheDocument());
    // userId 为 null → shortId 渲染 '—'（多处，取 getAllByText）
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
    // 被封禁条目展示"解封"按钮
    expect(screen.getByText('解封')).toBeInTheDocument();
  });

  it('recent 列表正常条目展示封禁按钮且 lastSeenAt 走 fmtAgo', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentNormal] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
    // 正常条目展示"封禁"按钮
    expect(screen.getByText('封禁')).toBeInTheDocument();
    // lastSeenAt 经 fmtAgo 渲染为「… 天前/小时前」
    expect(screen.getByText(/前$/)).toBeInTheDocument();
  });

  it('recent 列表可封禁设备（recent 分支的 onClick）', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentNormal] });
    mockApi.banClient.mockResolvedValue({ banned: true, deviceId: 'dev-recent-normal-abcdef' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockApi.banClient).toHaveBeenCalledWith('dev-recent-normal-abcdef', undefined));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('已封禁设备')));
  });

  it('recent 列表可解封设备（recent 分支的 onClick）', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentBannedNoUser] });
    mockApi.unbanClient.mockResolvedValue({ banned: false, deviceId: 'dev-recent-banned-1234567890' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('近期活跃')).toBeInTheDocument());
    fireEvent.click(screen.getByText('近期活跃'));
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(screen.getByText('解封设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认解封'));
    await waitFor(() => expect(mockApi.unbanClient).toHaveBeenCalledWith('dev-recent-banned-1234567890'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('已解封')));
  });

  it('SSE 行可打开遥测详情抽屉并加载明细', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    mockApi.getClientDetail.mockResolvedValue({
      deviceId: 'dev-short', platform: 'android', userId: 'u-short', appVersion: '1.2.0',
      model: 'Pixel', country: 'CN', firstSeenAt: '2026-04-01T00:00:00Z', lastSeenAt: '2026-04-15T10:00:00Z',
      isBanned: false, bannedAt: null, banReason: null, online: true, connectionCount: 3,
      telemetry: { total: 0, latest: null },
    });
    mockApi.getTelemetrySummary.mockResolvedValue({
      total: 5, firstTs: null, lastTs: null,
      byEventType: [{ eventType: 'session.summary', count: 5, avgDurationSecs: 10, totalErrors: 0, avgActionsPerMin: 2, avgResponseMs: 100 }],
      deviceProfile: null, featureUsage: [{ name: 'search', count: 3 }], routes: [{ name: '/learning', count: 4 }],
      clickTargets: [], totalClicks: 50, totalErrors: 0, totalDurationSecs: 300, sessionCount: 2,
    });
    mockApi.getTelemetry.mockResolvedValue({ records: [], total: 0 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('遥测详情')).toBeInTheDocument());
    fireEvent.click(screen.getByText('遥测详情'));
    // 抽屉标题
    await waitFor(() => expect(screen.getByText('设备遥测详情')).toBeInTheDocument());
    // 抽屉加载完成后展示遥测总览
    await waitFor(() => expect(screen.getByText(/遥测总览/)).toBeInTheDocument());
    expect(mockApi.getClientDetail).toHaveBeenCalledWith('dev-short');
    expect(mockApi.getTelemetrySummary).toHaveBeenCalledWith('dev-short');
  });

  it('unban 失败时弹错误 toast', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [{ ...shortIdEntry, isBanned: true }], recentlyActive: [] });
    mockApi.unbanClient.mockRejectedValue({ message: 'boom' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(screen.getByText('解封设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认解封'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith(expect.stringContaining('解封'), 'boom'));
  });

  it('拉取遥测调用 requestTelemetry 并弹成功 toast', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    mockApi.requestTelemetry.mockResolvedValue({ requestId: 'req-1' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('拉取遥测')).toBeInTheDocument());
    fireEvent.click(screen.getByText('拉取遥测'));
    await waitFor(() => expect(mockApi.requestTelemetry).toHaveBeenCalledWith('dev-short'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('遥测拉取请求')));
  });

  it('拉取遥测失败时弹错误 toast', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    mockApi.requestTelemetry.mockRejectedValue({ message: 'nope' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('拉取遥测')).toBeInTheDocument());
    fireEvent.click(screen.getByText('拉取遥测'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('遥测拉取失败', 'nope'));
  });

  it('取消封禁对话框后关闭且不调用 banClient', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('如:异常高频请求') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '临时' } });
    expect(input.value).toBe('临时');
    // 取消关闭弹窗，且不应发起封禁
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('封禁设备')).not.toBeInTheDocument());
    expect(mockApi.banClient).not.toHaveBeenCalled();
  });

  it('封禁带原因时调用 banClient 传 reason', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    mockApi.banClient.mockResolvedValue({ banned: true, deviceId: 'dev-short' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('如:异常高频请求') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '滥用' } });
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockApi.banClient).toHaveBeenCalledWith('dev-short', '滥用'));
  });

  it('刷新按钮在加载完成后可用并重新拉取', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('实时连接')).toBeInTheDocument());
    const refresh = screen.getByText('刷新').closest('button') as HTMLButtonElement;
    expect(refresh).not.toBeDisabled();
    const before = mockApi.getClients.mock.calls.length;
    fireEvent.click(refresh);
    await waitFor(() => expect(mockApi.getClients.mock.calls.length).toBeGreaterThan(before));
    // 刷新同时也会重拉分布
    await waitFor(() => expect(mockApi.getClientsDistribution.mock.calls.length).toBeGreaterThanOrEqual(2));
  });

  it('全部设备 tab 触发分页查询并渲染门控/版本分布面板', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [] });
    mockApi.getClientsPaginated.mockResolvedValue({
      data: [{
        deviceId: 'all-dev-1234567890ab', platform: 'web', userId: 'u-all', appVersion: '1.0.0',
        model: null, country: 'US', firstSeenAt: '2026-01-01T00:00:00Z', lastSeenAt: '2026-04-15T10:00:00Z',
        isBanned: false,
      }],
      total: 1, page: 1, perPage: 14, totalPages: 1,
    });
    await renderPage();
    await waitFor(() => expect(screen.getByText('全部设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('全部设备'));
    await waitFor(() => expect(mockApi.getClientsPaginated).toHaveBeenCalled());
    // 分页数据渲染：userId / 国家（避免与平台 select option 文本冲突）
    await waitFor(() => expect(screen.getByText('u-all')).toBeInTheDocument());
    expect(screen.getByText('US')).toBeInTheDocument();
    // 右下两块：版本分布与升级策略 / 全局版本门控
    expect(screen.getByText('版本分布与升级策略')).toBeInTheDocument();
    expect(screen.getByText('全局版本门控')).toBeInTheDocument();
  });
});
