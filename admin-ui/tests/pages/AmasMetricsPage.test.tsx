import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';
import { adminApi } from '@/api/admin';

// 重设计后的 AmasMetricsPage 是单滚动布局（无 tab），调用一组 adminApi 聚合端点。
// 逐一 mock 并给出与真实返回类型一致的安全默认值，避免未处理拒绝。
vi.mock('@/api/admin', () => ({
  adminApi: {
    amasMetricsKpi: vi.fn(),
    amasAlgorithmDistribution: vi.fn(),
    amasStageDistribution: vi.fn(),
    amasEloScatter: vi.fn(),
    amasMdmHeatmap: vi.fn(),
    amasFatigueTimeseries: vi.fn(),
    amasDecisionHistogram: vi.fn(),
    amasUserStateDistribution: vi.fn(),
    amasStateTransitions: vi.fn(),
    amasLearningClusters: vi.fn(),
    amasAnomalyFeed: vi.fn(),
    amasListVersions: vi.fn(),
    amasCompareVersionsExt: vi.fn(),
    amasMetricsTimeseries: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

const api = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

function histogram() {
  return { field: 'x', min: 0, max: 1, bins: [1, 2, 3, 2, 1], mean: 0.5, median: 0.5, sampleSize: 100 };
}

function primeDefaults() {
  api.amasMetricsKpi.mockResolvedValue({
    decisionTotal: 12345,
    hitCount: 9000,
    hitRate: 0.73,
    avgLatencyMs: 1.42,
    fatigueTriggerCount: 120,
    fatigueTriggerRate: 0.018,
    fatigueThreshold: 0.7,
  });
  api.amasAlgorithmDistribution.mockResolvedValue([
    { algorithm: 'ensemble', count: 5000, pct: 0.5 },
    { algorithm: 'sm2', count: 3000, pct: 0.3 },
    { algorithm: 'fsrs', count: 2000, pct: 0.2 },
  ]);
  api.amasStageDistribution.mockResolvedValue({
    totalUsers: 300,
    stages: [
      { stage: 'cold', users: 100, pct: 0.33, avgDecisions: 5, retention7d: 0.4, mainRoute: 'sm2' },
      { stage: 'transition', users: 100, pct: 0.33, avgDecisions: 20, retention7d: 0.6, mainRoute: 'fsrs' },
      { stage: 'stable', users: 100, pct: 0.34, avgDecisions: 80, retention7d: 0.8, mainRoute: 'ensemble' },
    ],
    trend: [
      { date: '2026-06-10', cold: 110, transition: 90, stable: 100 },
      { date: '2026-06-11', cold: 100, transition: 100, stable: 100 },
    ],
  });
  api.amasEloScatter.mockResolvedValue({
    total: 2,
    meanElo: 1500,
    points: [
      { elo: 1450, decisions: 30, deltaElo: 12 },
      { elo: 1600, decisions: 80, deltaElo: -5 },
    ],
  });
  api.amasMdmHeatmap.mockResolvedValue({
    days: ['2026-06-10', '2026-06-11'],
    bandCount: 3,
    cells: [
      [0.1, 0.2, 0.3],
      [0.4, 0.5, 0.6],
    ],
    peak: 0.6,
  });
  api.amasFatigueTimeseries.mockResolvedValue({
    points: [
      { date: '2026-06-10', avgFatigue: 0.3, peakFatigue: 0.7, triggerCount: 5 },
      { date: '2026-06-11', avgFatigue: 0.4, peakFatigue: 0.8, triggerCount: 8 },
    ],
    avgIntensity: 0.35,
    totalTriggers: 13,
    threshold: 0.7,
  });
  api.amasDecisionHistogram.mockResolvedValue({
    buckets: [
      { label: '0-10', count: 50 },
      { label: '11-50', count: 30 },
    ],
    p50: 12,
    p95: 80,
    totalUsers: 80,
  });
  api.amasUserStateDistribution.mockResolvedValue({
    attention: histogram(),
    fatigue: histogram(),
    motivation: histogram(),
    confidence: histogram(),
    coldStartExplore: 10,
    coldStartExploit: 20,
  });
  api.amasStateTransitions.mockResolvedValue({
    windowHours: 24,
    transitions: [{ from: 'cold', to: 'transition', count: 7 }],
  });
  api.amasLearningClusters.mockResolvedValue({
    k: 4,
    totalUsers: 300,
    clusters: [
      { label: '快速', count: 100, pct: 0.33, avgResponseMs: 800, errorRate: 0.1, recordsPerActiveDay: 30 },
      { label: '稳健', count: 200, pct: 0.67, avgResponseMs: 1200, errorRate: 0.2, recordsPerActiveDay: 20 },
    ],
  });
  api.amasAnomalyFeed.mockResolvedValue({
    summary: { error: 1, warn: 2, info: 3, invariantPass: 18, invariantTotal: 20, passRate: 0.9 },
    items: [
      {
        id: 'a1',
        timestamp: new Date().toISOString(),
        severity: 'error',
        code: 'AMAS-001',
        title: '命中率骤降',
        userId: 'u1',
        value: 0.2,
        expectedRange: '0.6-0.9',
        impactedUsers: 12,
        impactPct: 0.04,
      },
    ],
  });
  api.amasListVersions.mockResolvedValue([]);
  api.amasCompareVersionsExt.mockResolvedValue(null);
  api.amasMetricsTimeseries.mockResolvedValue([]);
}

describe('AmasMetricsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    primeDefaults();
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/AmasMetricsPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders page head, default day window and export button', async () => {
    await renderPage();
    // 页头主标题（h1.page-title）
    expect(screen.getByRole('heading', { name: 'AMAS 指标' })).toBeInTheDocument();
    // 时间窗 Seg：默认 7 天激活 + 14/30 天可选
    const seg7 = screen.getByRole('button', { name: '7 天' });
    expect(seg7).toBeInTheDocument();
    expect(seg7).toHaveClass('active');
    expect(screen.getByRole('button', { name: '14 天' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '30 天' })).toBeInTheDocument();
    // 导出按钮
    expect(screen.getByRole('button', { name: /导出/ })).toBeInTheDocument();
  });

  it('renders KPI cards and algorithm distribution panel', async () => {
    await renderPage();
    // KPI 卡：四张（决策总数 / 命中率 / 平均决策耗时 / 疲劳触发率）由 createResource 异步装载
    await waitFor(() => expect(screen.getByText('决策总数')).toBeInTheDocument());
    expect(screen.getByText('命中率')).toBeInTheDocument();
    expect(screen.getByText('平均决策耗时')).toBeInTheDocument();
    expect(screen.getByText('疲劳触发率')).toBeInTheDocument();
    expect(adminApi.amasMetricsKpi).toHaveBeenCalled();
    // 算法决策分布面板标题（h2.section-title）
    expect(screen.getByRole('heading', { name: '算法决策分布' })).toBeInTheDocument();
    await waitFor(() => expect(adminApi.amasAlgorithmDistribution).toHaveBeenCalled());
  });

  it('renders anomaly panel with severity badges and invariant pass-rate', async () => {
    await renderPage();
    expect(screen.getByRole('heading', { name: '异常检测' })).toBeInTheDocument();
    // anomalies 异步装载后：分级徽标 + 不变量通过率副标题
    await waitFor(() => expect(screen.getByText('1 error')).toBeInTheDocument());
    expect(screen.getByText('2 warn')).toBeInTheDocument();
    expect(screen.getByText('3 info')).toBeInTheDocument();
    // 副标题含通过率 90.0% (18/20)
    expect(screen.getByText(/不变量通过率 90\.0% \(18\/20\)/)).toBeInTheDocument();
    expect(adminApi.amasAnomalyFeed).toHaveBeenCalled();
  });

  it('renders user-state panels (decision histogram + learning clusters)', async () => {
    await renderPage();
    expect(screen.getByRole('heading', { name: '每用户决策数分布' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '学习风格聚类' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: '用户状态分布' })).toBeInTheDocument();
    // 聚类异步装载：标签文本出现
    await waitFor(() => expect(screen.getByText('快速')).toBeInTheDocument());
    expect(screen.getByText('稳健')).toBeInTheDocument();
    expect(adminApi.amasLearningClusters).toHaveBeenCalled();
    expect(adminApi.amasDecisionHistogram).toHaveBeenCalled();
  });

  it('renders version-compare panel with empty state when fewer than two versions', async () => {
    // amasListVersions 默认 [] → 版本不足两个 → 提示空态
    await renderPage();
    expect(screen.getByRole('heading', { name: '版本对比' })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByText('选择两个版本以对比指标切片')).toBeInTheDocument(),
    );
    expect(adminApi.amasListVersions).toHaveBeenCalled();
  });
});
