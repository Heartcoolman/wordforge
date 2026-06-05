import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from './helpers/render';

// ─────────────────────────────────────────────────────────────────────────
// big_pages 覆盖簇:AmasAdvisorPage / DashboardPage / UserManagementPage
// 用 Proxy 兜底 mock 全部 adminApi / amasApi 方法(默认 resolve),再对真正影响
// 渲染分支的方法 override。EChart stub 成占位 div。绝不真实网络。
// ─────────────────────────────────────────────────────────────────────────

// 默认对象:既能当 paginated（含 data/total）又含 items/rows/workers 等兜底字段。
const DEFAULT_OBJ = () => ({
  data: [], total: 0, items: [], rows: [], workers: [],
  suggestions: [], versions: [], sessions: [], devices: [], entries: [],
  page: 1, perPage: 20, totalPages: 0, results: [],
});

// adminApi:Proxy 兜底所有方法;cache 保证同名方法返回同一个 vi.fn(可 override)。
const adminCache: Record<string, ReturnType<typeof vi.fn>> = {};
vi.mock('@/api/admin', () => ({
  adminApi: new Proxy({}, {
    get: (_t, p) => {
      if (typeof p !== 'string') return undefined;
      if (!adminCache[p]) adminCache[p] = vi.fn(() => Promise.resolve(DEFAULT_OBJ()));
      return adminCache[p];
    },
  }),
}));

const amasCache: Record<string, ReturnType<typeof vi.fn>> = {};
vi.mock('@/api/amas', () => ({
  amasApi: new Proxy({}, {
    get: (_t, p) => {
      if (typeof p !== 'string') return undefined;
      if (!amasCache[p]) amasCache[p] = vi.fn(() => Promise.resolve([]));
      return amasCache[p];
    },
  }),
}));

// EChart stub —— 主动执行 option() 构建器（执行图表 builder 覆盖行），但不碰 echarts。
vi.mock('@/components/ui/EChart', () => ({
  EChart: (props: { option?: () => unknown }) => {
    try { props.option?.(); } catch { /* 构建器抛错不阻塞测试 */ }
    return <div data-testid="chart" />;
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import { uiStore } from '@/stores/ui';
const api = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const amas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const toast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

function resetCaches() {
  vi.clearAllMocks();
  for (const k of Object.keys(adminCache)) {
    adminCache[k].mockReset();
    adminCache[k].mockResolvedValue(DEFAULT_OBJ());
  }
  for (const k of Object.keys(amasCache)) {
    amasCache[k].mockReset();
    amasCache[k].mockResolvedValue([]);
  }
}

// ─────────────────────────── AmasAdvisorPage ───────────────────────────
const COST = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 42.1, forecastYuan: 6.84,
  avg7dCostYuan: 0.14, monthCalls: 31, acceptedCount: 47, rejectedCount: 6,
  acceptanceRate: 0.887, usdToCny: 7.2,
};
const CFG = {
  model: 'deepseek-v2', pollCron: '0 */20 * * * *', apiKeyTail: 'f3a8', monthCapYuan: 10,
  autoApplyEnabled: false, autoApplyMaxPerDay: 1, autoApplyMinConfidence: 0.8,
  grayscaleSteps: [20, 60, 100], advisorEnabled: true,
};
const SUGG = (id: number, status: string) => ({
  id, createdAt: `2026-06-0${id}T00:00:00Z`, basedOnVersionHash: 'abc123def456',
  patchJson: { 'wordSelector.alpha': 0.5 }, rationale: `调参建议 ${id}`,
  evidenceJson: {}, status, decidedBy: null, decidedAt: null, decisionNote: null,
  costUsd: 0.012, tokensInput: 100, tokensOutput: 50, confidence: 0.9,
  baseValuesJson: { 'wordSelector.alpha': 0.4 },
});
const CANARY = {
  id: 7, suggestionId: 3, percent: 20, status: 'active', versionHash: 'cafebabe00deadbeef',
  createdAt: '2026-06-03T00:00:00Z', patchJson: { 'wordSelector.alpha': 0.5 },
  rationale: '灰度建议', baseValuesJson: { 'wordSelector.alpha': 0.4 },
  cohortLo: 0, cohortHi: 20, liveReward: 0.82, baselineReward: 0.8, liveAnomalyRate: 0.01,
};

function primeAdvisor() {
  api.amasAdvisorCost.mockResolvedValue(COST);
  api.amasAdvisorCostDaily.mockResolvedValue([{ date: '2026-06-01', costYuan: 0.1, calls: 2 }]);
  api.amasAdvisorConfig.mockResolvedValue(CFG);
  api.amasListWhitelist.mockResolvedValue([]);
  // amasListSuggestions 按 status 分发:供 page + HistoryTable 共用,默认空数组
  api.amasListSuggestions.mockImplementation((status?: string) => {
    if (status === 'pending') return Promise.resolve([SUGG(1, 'pending')]);
    if (status === 'approved') return Promise.resolve([SUGG(2, 'approved')]);
    if (status === 'auto_applied') return Promise.resolve([]);
    if (status === 'rejected') return Promise.resolve([SUGG(4, 'rejected')]);
    return Promise.resolve([]); // HistoryTable(undefined status)
  });
  api.amasListCanaries.mockResolvedValue([CANARY]);
}

async function renderAdvisor() {
  const { default: Page } = await import('@/pages/AmasAdvisorPage');
  return renderWithProviders(() => <Page />);
}

describe('AmasAdvisorPage — tabs / decisions / header ops', () => {
  beforeEach(() => { resetCaches(); primeAdvisor(); });

  it('渲染标题 + 成本行 + 待审建议', async () => {
    await renderAdvisor();
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('¥4.21')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('调参建议 1')).toBeInTheDocument());
  });

  it('批准建议触发 amasApproveSuggestion', async () => {
    api.amasApproveSuggestion.mockResolvedValue({ versionHash: 'newhash1234' });
    await renderAdvisor();
    fireEvent.click(await screen.findByText('批准并应用'));
    await waitFor(() => expect(api.amasApproveSuggestion).toHaveBeenCalledWith(1));
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
  });

  it('拒绝建议触发 amasRejectSuggestion', async () => {
    api.amasRejectSuggestion.mockResolvedValue(undefined);
    await renderAdvisor();
    fireEvent.click(await screen.findByText('拒绝'));
    await waitFor(() => expect(api.amasRejectSuggestion).toHaveBeenCalledWith(1));
  });

  it('进灰度触发 amasCreateCanary', async () => {
    api.amasCreateCanary.mockResolvedValue(undefined);
    await renderAdvisor();
    fireEvent.click(await screen.findByText('进灰度 20%'));
    await waitFor(() => expect(api.amasCreateCanary).toHaveBeenCalledWith({ suggestionId: 1, percent: 20 }));
  });

  it('批准失败走错误 toast', async () => {
    api.amasApproveSuggestion.mockRejectedValue(new Error('boom'));
    await renderAdvisor();
    fireEvent.click(await screen.findByText('批准并应用'));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
  });

  it('切到灰度中 tab 显示 canary 卡', async () => {
    await renderAdvisor();
    const canaryTab = await screen.findByRole('tab', { name: /灰度中/ });
    fireEvent.click(canaryTab);
    // PatchCanaryCard 渲染 "建议 #3 · cafebabe00" + "灰度 20%" + "回滚" 按钮
    await waitFor(() => expect(screen.getByText('回滚')).toBeInTheDocument());
    expect(screen.getByText(/灰度 20%/)).toBeInTheDocument();
  });

  it('切到已生效 tab 显示 approved 建议', async () => {
    await renderAdvisor();
    const tab = await screen.findByRole('tab', { name: /已生效/ });
    fireEvent.click(tab);
    await waitFor(() => expect(screen.getByText('调参建议 2')).toBeInTheDocument());
  });

  it('切到已拒绝 tab 显示 rejected 建议', async () => {
    await renderAdvisor();
    const tab = await screen.findByRole('tab', { name: /已拒绝/ });
    fireEvent.click(tab);
    await waitFor(() => expect(screen.getByText('调参建议 4')).toBeInTheDocument());
  });

  it('立即触发巡查 调用 amasAdvisorRun（有新建议）', async () => {
    api.amasAdvisorRun.mockResolvedValue({ produced: true });
    await renderAdvisor();
    fireEvent.click(await screen.findByText('立即触发巡查'));
    await waitFor(() => expect(api.amasAdvisorRun).toHaveBeenCalled());
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('巡查完成，产出新建议'));
  });

  it('立即触发巡查 失败走错误 toast', async () => {
    api.amasAdvisorRun.mockRejectedValue(new Error('x'));
    await renderAdvisor();
    fireEvent.click(await screen.findByText('立即触发巡查'));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith('触发失败', 'x'));
  });

  it('接受全部待审 调用 amasApproveAllSuggestions', async () => {
    api.amasApproveAllSuggestions.mockResolvedValue({ results: [{ ok: true }, { ok: false }] });
    await renderAdvisor();
    const btn = await screen.findByText(/接受全部待审/);
    fireEvent.click(btn);
    await waitFor(() => expect(api.amasApproveAllSuggestions).toHaveBeenCalled());
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith('已批准 1/2 条'));
  });

  it('切换自动巡查开关 调用 amasUpdateAdvisorConfig', async () => {
    api.amasUpdateAdvisorConfig.mockResolvedValue(undefined);
    await renderAdvisor();
    const sw = await screen.findByLabelText('自动巡查');
    fireEvent.click(sw);
    await waitFor(() => expect(api.amasUpdateAdvisorConfig).toHaveBeenCalledWith({ advisorEnabled: false }));
  });

  it('成本接口失败时成本行降级而不整页崩', async () => {
    api.amasAdvisorCost.mockRejectedValue(new Error('boom'));
    await renderAdvisor();
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/成本信息加载失败/)).toBeInTheDocument());
  });
});

// ─────────────────────────── DashboardPage ───────────────────────────
function primeDashboard() {
  api.getStats.mockResolvedValue({ users: 100, words: 5000, records: 10000, trend: { users: { value: 5, label: '较昨日' } } });
  api.getEngagement.mockResolvedValue({ totalUsers: 100, activeToday: 12, retentionRate: 0.12, trend: { activeToday: { value: 2, label: '较昨日' } } });
  api.getStudyOverview.mockResolvedValue({
    summary: {
      totalDurationSecs: 7200, sessionCount: 30, recordCount: 500, correctCount: 410,
      accuracy: 0.82, newWords: 45, recordCountDeltaPct: 0.1, accuracyDeltaPt: 0.02, prevAccuracy: 0.8,
    },
    daily: [],
  });
  api.getDailyActiveUsers.mockResolvedValue([{ date: '2026-06-01', count: 12, registered: 3 }]);
  api.getDailyRecords.mockResolvedValue([{ date: '2026-06-01', correct: 50, total: 60, durationSecs: 600, newWords: 8 }]);
  api.getHealth.mockResolvedValue({
    status: 'healthy', dbSizeBytes: 1048576, uptimeSecs: 7200, version: '1.1.4',
    resources: { cpuPct: 12.5, memoryRssBytes: 5e7, pool: { connections: 2, max: 8 }, diskTotalBytes: 1e10, diskFreeBytes: 5e9 },
  });
  api.checkUpdate.mockResolvedValue({ currentVersion: '1.1.4', latestVersion: '1.1.4', hasUpdate: false, releaseUrl: null });
  api.amasMetricsTimeseries.mockResolvedValue([
    { algorithm: 'Heuristic', callCount: 30 },
    { algorithm: 'MDM', callCount: 10 },
  ]);
  api.analyticsHourly.mockResolvedValue({ matrix: Array.from({ length: 7 }, () => Array.from({ length: 24 }, () => 1)) });
  api.monitoringWorkers.mockResolvedValue({
    workers: [
      { workerName: 'heartbeat', lastOutcome: 'ok', lastRunAt: '2026-06-01T00:00:00Z', lastDurationMs: 12, lastError: null },
      { workerName: 'advisor', lastOutcome: 'error', lastRunAt: '2026-06-01T00:00:00Z', lastDurationMs: 30, lastError: 'fail' },
    ],
  });
  api.amasListSuggestions.mockResolvedValue([SUGG(1, 'pending')]);
  api.listFeedback.mockResolvedValue({ data: [], total: 3, page: 1, perPage: 1, totalPages: 3 });
  amas.getMonitoring.mockResolvedValue([{ eventType: 'error_x' }, { eventType: 'info_y' }]);
}

async function renderDashboard() {
  const { default: Page } = await import('@/pages/DashboardPage');
  return renderWithProviders(() => <Page />);
}

describe('DashboardPage — KPI / rows / refresh / export', () => {
  beforeEach(() => { resetCaches(); primeDashboard(); });

  it('渲染 KPI + worker 网格 + 待办事项', async () => {
    await renderDashboard();
    await waitFor(() => expect(screen.getByText('全局概览')).toBeInTheDocument());
    // worker 心跳网格标题(2 个 tokio worker)
    await waitFor(() => expect(screen.getByText('Worker 心跳')).toBeInTheDocument());
    // 待办:未处理反馈(total=3) + LLM 待审建议
    await waitFor(() => expect(screen.getByText(/3 条未处理反馈/)).toBeInTheDocument());
  });

  it('刷新按钮重新拉取并 toast', async () => {
    await renderDashboard();
    await waitFor(() => expect(screen.getByText('全局概览')).toBeInTheDocument());
    api.getStats.mockClear();
    fireEvent.click(screen.getByText('刷新'));
    await waitFor(() => expect(api.getStats).toHaveBeenCalled());
    expect(toast.info).toHaveBeenCalledWith('已刷新所有数据');
  });

  it('导出按钮生成 CSV 并 toast', async () => {
    const createUrl = vi.fn(() => 'blob:x');
    const revoke = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: createUrl });
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revoke });
    await renderDashboard();
    await waitFor(() => expect(screen.getByText('全局概览')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导出'));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(expect.stringContaining('CSV')));
    expect(createUrl).toHaveBeenCalled();
  });

  it('切换天窗口为 14 天重拉 study-overview', async () => {
    await renderDashboard();
    await waitFor(() => expect(screen.getByText('全局概览')).toBeInTheDocument());
    const btn14 = screen.getAllByRole('radio').find((b) => b.textContent === '14天');
    expect(btn14).toBeDefined();
    fireEvent.click(btn14!);
    await waitFor(() => expect(api.getStudyOverview).toHaveBeenCalledWith(14));
  });

  it('健康降级时显示性能降级', async () => {
    api.getHealth.mockResolvedValue({ status: 'degraded', dbSizeBytes: 1024, uptimeSecs: 100, version: '1.1.4' });
    await renderDashboard();
    await waitFor(() => expect(screen.getAllByText('性能降级').length).toBeGreaterThan(0));
  });

  it('有新版本时待办出现升级行', async () => {
    api.checkUpdate.mockResolvedValue({ currentVersion: '1.1.3', latestVersion: '1.1.4', hasUpdate: true, releaseUrl: 'https://x' });
    await renderDashboard();
    await waitFor(() => expect(screen.getAllByText(/新版本 1.1.4/).length).toBeGreaterThan(0));
  });
});

// ─────────────────────────── UserManagementPage ───────────────────────────
const baseUser = {
  failedLoginCount: 0, lockedUntil: null, createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-01T00:00:00Z', role: 'user' as const, status: 'active' as const, lastLoginAt: null,
};
const u1 = { ...baseUser, id: '1', username: 'alice', email: 'alice@example.com', isBanned: false, stats: { recordCount: 10, correctCount: 8, last20Outcomes: [1, 0, 1] } };
const u2 = { ...baseUser, id: '2', username: 'bob', email: 'bob@example.com', isBanned: true, status: 'suspended' as const };

function primeUsers() {
  api.getStats.mockResolvedValue({ users: 100, words: 0, records: 0, trend: { users: { value: 5, label: '较昨日' } } });
  api.getEngagement.mockResolvedValue({ totalUsers: 100, activeToday: 12, retentionRate: 0.12 });
  api.getDailyActiveUsers.mockResolvedValue([{ date: '2026-05-30', count: 10, registered: 3 }]);
  api.userFacets.mockResolvedValue({ total: 100, active: 80, inactive7d: 12, banned: 5, admins: 3 });
  api.userProfile.mockResolvedValue({ userId: '1', totalRecords: 10, correctRecords: 8, accuracy: 0.8, avgResponseTimeMs: 1500, sessionCount: 5, wordbookDistribution: [{ name: 'CET4', recordCount: 5 }] });
  api.userSessions.mockResolvedValue({ sessions: [] });
  api.userDevices.mockResolvedValue({ devices: [{ deviceId: 'dev1234567890abcd', platform: 'Web', appVersion: '1.0', isBanned: false, lastSeenAt: '2026-06-01T00:00:00Z' }] });
  api.userAuditLog.mockResolvedValue({ entries: [] });
  api.userExtras.mockResolvedValue({
    preferences: null, elo: null, habit: null, streakDays: 0, latestStrategy: null, latestDevice: null,
    selectedWordbooks: [], metrics7d: { records: 0, correct: 0, accuracy: null, fatigueAlertCount: 0 },
  });
  api.userActivityLog.mockResolvedValue({ entries: [] });
  api.getUsers.mockResolvedValue({ data: [u1, u2], total: 2 });
}

async function renderUsers() {
  const { default: Page } = await import('@/pages/UserManagementPage');
  return renderWithProviders(() => <Page />);
}

describe('UserManagementPage — list / chips / drawer / create / bulk', () => {
  beforeEach(() => { resetCaches(); primeUsers(); });

  it('渲染用户列表与 KPI header', async () => {
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    expect(screen.getByText('bob')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/本周新增/)).toBeInTheDocument());
  });

  it('切 chip 到禁用 重拉 banned=true', async () => {
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('禁用'));
    await waitFor(() => expect(api.getUsers).toHaveBeenLastCalledWith(expect.objectContaining({ banned: true })));
  });

  it('切 chip 到 7 天未登录 重拉 inactiveDays=7', async () => {
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('7 天未登录'));
    await waitFor(() => expect(api.getUsers).toHaveBeenLastCalledWith(expect.objectContaining({ inactiveDays: 7 })));
  });

  it('点行打开 Drawer 显示资料 tab', async () => {
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('alice'));
    await waitFor(() => expect(screen.getByText('用户详情')).toBeInTheDocument());
    expect(screen.getByText('答题档案')).toBeInTheDocument();
  });

  it('Drawer 切到答题档案 tab 显示词书分布', async () => {
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('alice'));
    await waitFor(() => expect(screen.getByText('答题档案')).toBeInTheDocument());
    fireEvent.click(screen.getByText('答题档案'));
    await waitFor(() => expect(screen.getByText('CET4')).toBeInTheDocument());
  });

  it('Drawer 切到设备会话 tab 显示设备并可封设备', async () => {
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('alice'));
    await waitFor(() => expect(screen.getByText('设备会话')).toBeInTheDocument());
    fireEvent.click(screen.getByText('设备会话'));
    await waitFor(() => expect(screen.getByText('封设备')).toBeInTheDocument());
    fireEvent.click(screen.getByText('封设备'));
    await waitFor(() => expect(screen.getByText('封禁设备')).toBeInTheDocument());
  });

  it('打开添加用户 Modal 并提交 createUser', async () => {
    api.createUser.mockResolvedValue({ ...baseUser, id: '99', username: 'newuser', email: 'new@x.com', isBanned: false });
    await renderUsers();
    await waitFor(() => expect(screen.getByText('+ 添加用户')).toBeInTheDocument());
    fireEvent.click(screen.getByText('+ 添加用户'));
    await waitFor(() => expect(screen.getByText('添加用户')).toBeInTheDocument());
    const email = document.querySelector('input[type="email"]') as HTMLInputElement;
    const pw = document.querySelectorAll('input[type="password"]');
    const textInputs = Array.from(document.querySelectorAll('input[type="text"], input:not([type])')) as HTMLInputElement[];
    const usernameInput = textInputs[textInputs.length - 1];
    fireEvent.input(email, { target: { value: 'new@x.com' } });
    fireEvent.input(usernameInput, { target: { value: 'newuser' } });
    fireEvent.input(pw[0], { target: { value: 'NewPass123' } });
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() => expect(api.createUser).toHaveBeenCalled());
  });

  it('添加用户必填校验', async () => {
    await renderUsers();
    await waitFor(() => expect(screen.getByText('+ 添加用户')).toBeInTheDocument());
    fireEvent.click(screen.getByText('+ 添加用户'));
    await waitFor(() => expect(screen.getByText('添加用户')).toBeInTheDocument());
    fireEvent.click(screen.getByText('创建'));
    await waitFor(() => expect(screen.getByText('邮箱 / 用户名 / 密码均为必填')).toBeInTheDocument());
  });

  it('多选后批量封禁', async () => {
    api.usersBulkBan.mockResolvedValue({ succeeded: 2, failed: 0 });
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('全选 / 全不选'));
    fireEvent.click(await screen.findByText('批量封禁'));
    await waitFor(() => expect(api.usersBulkBan).toHaveBeenCalled());
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
  });

  it('多选后批量重置密码', async () => {
    api.usersBulkResetPassword.mockResolvedValue({ succeeded: 2, failed: 0 });
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByLabelText('全选 / 全不选'));
    fireEvent.click(await screen.findByText('重置密码'));
    await waitFor(() => expect(api.usersBulkResetPassword).toHaveBeenCalled());
  });

  it('导出 CSV 列表', async () => {
    const createUrl = vi.fn(() => 'blob:x');
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: createUrl });
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: vi.fn() });
    await renderUsers();
    await waitFor(() => expect(screen.getByText('alice')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导出 CSV'));
    await waitFor(() => expect(toast.success).toHaveBeenCalledWith(expect.stringContaining('已导出')));
  });

  it('加载失败走错误 toast', async () => {
    api.getUsers.mockRejectedValue(new Error('500'));
    await renderUsers();
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
  });

  it('空列表显示空态', async () => {
    api.getUsers.mockResolvedValue({ data: [], total: 0 });
    await renderUsers();
    await waitFor(() => expect(screen.getByText('暂无用户')).toBeInTheDocument());
  });
});
