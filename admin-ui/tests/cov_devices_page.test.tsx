import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from './helpers/render';

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

// 短 ID（<=12 字符）走 truncateId 的非截断分支
const shortIdEntry = {
  deviceId: 'dev-short',
  platform: 'android',
  userId: 'u-short',
  connectedSecs: 600,
  connectionCount: 3,
  dataChannels: { amas: 'uploaded' as const, learning: 'nil' as const, telemetry: 'none' as const },
  isBanned: false,
};

// recent 列表条目：userId 为 null（走 '-' 分支）、isBanned=true（走"已封禁"徽标）
const recentBannedNoUser = {
  deviceId: 'dev-recent-banned-1234567890',
  platform: 'ios',
  userId: null,
  lastSeenAt: '2026-04-15T10:00:00Z', // ISO 含 T，走 formatTimestamp 的 T 分支
  isBanned: true,
  dataChannels: { amas: 'none' as const, learning: 'uploaded' as const, telemetry: 'nil' as const },
};

// recent 列表条目：正常状态、带 userId、lastSeenAt 为非法字符串（走 isNaN 回退原串分支）
const recentNormal = {
  deviceId: 'dev-recent-normal-abcdef',
  platform: 'macos',
  userId: 'usr-recent-normal',
  lastSeenAt: 'not-a-real-date',
  isBanned: false,
  dataChannels: { amas: 'uploaded' as const, learning: 'uploaded' as const, telemetry: 'uploaded' as const },
};

// 完整遥测记录（所有 Show 分支命中）
const fullTelemetry = {
  id: 't-full',
  deviceId: 'd',
  userId: 'u',
  eventType: 'session.summary',
  serverTs: '2026-04-15 10:00:00', // 老格式（空格），走 replace 分支
  deviceProfile: {
    cpuCores: 8, memoryGb: 16, screenWidth: 1920, screenHeight: 1080, pixelRatio: 2,
    osName: 'macOS', browserName: 'Chrome', browserVersion: '120',
    timezone: 'Asia/Shanghai', language: 'zh-CN', touchSupport: false, onlineStatus: true,
  },
  sessionStats: { sessionDurationSecs: 300, actionsPerMin: 12.5, errorCount: 0, avgResponseTimeMs: 120.4 },
  behaviorSummary: {
    currentRoute: '/learning', clickCount: 50, clickTargets: null,
    scrollDepthPct: 80.5, visibilityChanges: 1, routeChanges: 3,
  },
  featureUsage: { search: 5, edit: 2 },
};

// 最简遥测记录：deviceProfile 全 null（不渲染设备信息块），behaviorSummary.currentRoute=null
// （不渲染行为摘要块），featureUsage 为空（不渲染功能使用块）
const minimalTelemetry = {
  id: 't-min',
  deviceId: 'd',
  userId: null,
  eventType: 'app.error',
  serverTs: '',
  deviceProfile: {
    cpuCores: null, memoryGb: null, screenWidth: null, screenHeight: null, pixelRatio: null,
    osName: null, browserName: null, browserVersion: null,
    timezone: null, language: null, touchSupport: null, onlineStatus: null,
  },
  sessionStats: { sessionDurationSecs: 10, actionsPerMin: 0, errorCount: 3, avgResponseTimeMs: 0 },
  behaviorSummary: {
    currentRoute: null, clickCount: null, clickTargets: null,
    scrollDepthPct: null, visibilityChanges: null, routeChanges: null,
  },
  featureUsage: {},
};

describe('DevicesPage 覆盖补充', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/DevicesPage');
    return renderWithProviders(() => <Page />);
  }

  it('SSE 列表短 ID 原样显示且渲染连接时长/连接数', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    await renderPage();
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
    await waitFor(() => expect(screen.getByText(/SSE 实时连接 \(1\)/)).toBeInTheDocument());
    expect(screen.getByText(/近期活跃 \(1\)/)).toBeInTheDocument();
  });

  it('recent 列表渲染已封禁徽标、userId 为 null 显示 -、ISO 时间格式化', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentBannedNoUser] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('ios')).toBeInTheDocument());
    // 已封禁徽标
    expect(screen.getByText('已封禁')).toBeInTheDocument();
    // userId 为 null → '-'
    expect(screen.getByText('-')).toBeInTheDocument();
    // 被封禁条目展示"解封"按钮
    expect(screen.getByText('解封')).toBeInTheDocument();
  });

  it('recent 列表正常条目展示正常徽标且非法时间回退原串', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentNormal] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('macos')).toBeInTheDocument());
    expect(screen.getByText('正常')).toBeInTheDocument();
    // 非法时间字符串原样回退
    expect(screen.getByText('not-a-real-date')).toBeInTheDocument();
    // 正常条目展示"封禁"按钮
    expect(screen.getByText('封禁')).toBeInTheDocument();
  });

  it('recent 列表可封禁设备（recent 分支的 onClick）', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentNormal] });
    mockApi.banClient.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockApi.banClient).toHaveBeenCalledWith('dev-recent-normal-abcdef', undefined));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('已封禁设备')));
  });

  it('recent 列表可解封设备（recent 分支的 onClick）', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentBannedNoUser] });
    mockApi.unbanClient.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(screen.getByText('确认解封设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认解封'));
    await waitFor(() => expect(mockApi.unbanClient).toHaveBeenCalledWith('dev-recent-banned-1234567890'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalledWith(expect.stringContaining('已解封设备')));
  });

  it.skip('recent 列表"历史"按钮加载遥测面板', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [], recentlyActive: [recentNormal] });
    mockApi.getTelemetry.mockResolvedValue({ records: [fullTelemetry], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/近期活跃/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/近期活跃/));
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText(/遥测记录/)).toBeInTheDocument());
    expect(screen.getByText('session.summary')).toBeInTheDocument();
  });

  it('unban 失败时弹错误 toast', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [{ ...shortIdEntry, isBanned: true }], recentlyActive: [] });
    mockApi.unbanClient.mockRejectedValue({ message: 'boom' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('解封'));
    await waitFor(() => expect(screen.getByText('确认解封')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认解封'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith(expect.stringContaining('解封'), 'boom'));
  });

  it.skip('完整遥测记录渲染所有分区（设备信息/会话统计/行为摘要/功能使用）', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockResolvedValue({ records: [fullTelemetry], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    await waitFor(() => expect(screen.getByText('设备信息')).toBeInTheDocument());
    expect(screen.getByText('会话统计')).toBeInTheDocument();
    expect(screen.getByText('行为摘要')).toBeInTheDocument();
    expect(screen.getByText('功能使用')).toBeInTheDocument();
    // 设备信息字段
    expect(screen.getByText('系统')).toBeInTheDocument();
    expect(screen.getByText('macOS')).toBeInTheDocument();
    expect(screen.getByText('浏览器')).toBeInTheDocument();
    expect(screen.getByText('CPU')).toBeInTheDocument();
    expect(screen.getByText('分辨率')).toBeInTheDocument();
    expect(screen.getByText('时区')).toBeInTheDocument();
    // 行为摘要路由
    expect(screen.getByText('/learning')).toBeInTheDocument();
    // 功能使用 key
    expect(screen.getByText('search')).toBeInTheDocument();
    expect(screen.getByText('edit')).toBeInTheDocument();
  });

  it.skip('最简遥测记录隐藏设备信息/行为摘要/功能使用分区', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    mockApi.getTelemetry.mockResolvedValue({ records: [minimalTelemetry], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    // 会话统计始终渲染
    await waitFor(() => expect(screen.getByText('会话统计')).toBeInTheDocument());
    expect(screen.getByText('app.error')).toBeInTheDocument();
    // deviceProfile 全 null → 无设备信息分区
    expect(screen.queryByText('设备信息')).not.toBeInTheDocument();
    // currentRoute=null → 无行为摘要分区
    expect(screen.queryByText('行为摘要')).not.toBeInTheDocument();
    // featureUsage 为空 → 无功能使用分区
    expect(screen.queryByText('功能使用')).not.toBeInTheDocument();
  });

  it('拉取遥测期间按钮禁用（requestingTelemetry 分支）', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    // 用一个 pending 的 promise 让 requestingTelemetry 维持非空
    let resolveReq: (v?: unknown) => void = () => {};
    mockApi.requestTelemetry.mockReturnValue(new Promise((res) => { resolveReq = res; }));
    await renderPage();
    await waitFor(() => expect(screen.getByText('拉取遥测')).toBeInTheDocument());
    const btn = screen.getByText('拉取遥测') as HTMLButtonElement;
    fireEvent.click(btn);
    // 点击后进入 requesting 态，按钮 disabled
    await waitFor(() => expect(btn).toBeDisabled());
    resolveReq();
    await waitFor(() => expect(btn).not.toBeDisabled());
  });

  it.skip('遥测面板加载中显示 Spinner 再渲染内容', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    let resolveTel: (v: { records: unknown[]; total: number }) => void = () => {};
    mockApi.getTelemetry.mockReturnValue(new Promise((res) => { resolveTel = res; }));
    await renderPage();
    await waitFor(() => expect(screen.getByText('历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('历史'));
    // 面板已出现（标题在），但内容仍在加载
    await waitFor(() => expect(screen.getByText(/遥测记录/)).toBeInTheDocument());
    resolveTel({ records: [fullTelemetry], total: 1 });
    await waitFor(() => expect(screen.getByText('session.summary')).toBeInTheDocument());
  });

  it('取消封禁对话框后重置原因输入', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('封禁原因（可选）') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '临时' } });
    expect(input.value).toBe('临时');
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认封禁设备')).not.toBeInTheDocument());
    // 重新打开应为空（banReason 被重置）
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    const input2 = screen.getByPlaceholderText('封禁原因（可选）') as HTMLInputElement;
    expect(input2.value).toBe('');
  });

  it('封禁带原因时调用 banClient 传 reason', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    mockApi.banClient.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByText('封禁')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封禁'));
    await waitFor(() => expect(screen.getByText('确认封禁设备')).toBeInTheDocument());
    const input = screen.getByPlaceholderText('封禁原因（可选）') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '滥用' } });
    fireEvent.click(screen.getByText('确认封禁'));
    await waitFor(() => expect(mockApi.banClient).toHaveBeenCalledWith('dev-short', '滥用'));
  });

  it('刷新按钮在加载完成后可用并重新拉取', async () => {
    mockApi.getClients.mockResolvedValue({ sseLive: [shortIdEntry], recentlyActive: [] });
    await renderPage();
    await waitFor(() => expect(screen.getByText(/SSE 实时连接/)).toBeInTheDocument());
    const refresh = screen.getByText('刷新') as HTMLButtonElement;
    expect(refresh).not.toBeDisabled();
    fireEvent.click(refresh);
    await waitFor(() => expect(mockApi.getClients).toHaveBeenCalledTimes(2));
  });

  it.skip('关闭遥测面板后再次打开不同设备', async () => {
    mockApi.getClients.mockResolvedValue({
      sseLive: [shortIdEntry, { ...shortIdEntry, deviceId: 'dev-other-1234567890ab', userId: 'u2' }],
      recentlyActive: [],
    });
    mockApi.getTelemetry.mockResolvedValue({ records: [fullTelemetry], total: 1 });
    await renderPage();
    await waitFor(() => expect(screen.getAllByText('历史').length).toBe(2));
    const historyButtons = screen.getAllByText('历史');
    fireEvent.click(historyButtons[0]);
    await waitFor(() => expect(screen.getByText('关闭')).toBeInTheDocument());
    fireEvent.click(screen.getByText('关闭'));
    await waitFor(() => expect(screen.queryByText('关闭')).not.toBeInTheDocument());
    // 再开第二个设备
    fireEvent.click(screen.getAllByText('历史')[1]);
    await waitFor(() => expect(screen.getByText(/遥测记录/)).toBeInTheDocument());
    expect(mockApi.getTelemetry).toHaveBeenCalledTimes(2);
  });
});