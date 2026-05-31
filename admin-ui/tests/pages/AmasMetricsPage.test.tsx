import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    // metrics 面板（MetricsDashboard 及其子面板）
    amasMetricsTimeseries: vi.fn().mockResolvedValue([]),
    amasMetricsKpi: vi.fn().mockResolvedValue({
      decisionTotal: 0,
      hitRate: 0,
      avgLatencyMs: 0,
      fatigueTriggerRate: 0,
    }),
    amasAlgorithmDistribution: vi.fn().mockResolvedValue([]),
    amasStageDistribution: vi.fn().mockResolvedValue({ totalUsers: 0, stages: [], trend: [] }),
    amasEloScatter: vi.fn().mockResolvedValue({ total: 0, meanElo: 0, points: [] }),
    amasMdmHeatmap: vi.fn().mockResolvedValue({ days: [], cells: [], peak: 0 }),
    amasFatigueTimeseries: vi.fn().mockResolvedValue({
      points: [],
      threshold: 0,
      avgIntensity: 0,
      totalTriggers: 0,
    }),
    amasDecisionHistogram: vi.fn().mockResolvedValue({ totalUsers: 0, buckets: [], p50: 0, p95: 0 }),
    // 异常 / 不变量面板
    amasAnomalyFeed: vi.fn().mockResolvedValue({
      summary: { error: 0, warn: 0, info: 0, invariantPass: 0, invariantTotal: 0, passRate: 0 },
      items: [],
    }),
    // 版本对比面板
    amasListVersions: vi.fn().mockResolvedValue([]),
    amasCompareVersionsExt: vi.fn().mockResolvedValue(null),
    // 用户状态分布面板
    amasStateTransitions: vi.fn().mockResolvedValue({ transitions: [] }),
    amasLearningClusters: vi.fn().mockResolvedValue({ clusters: [] }),
  },
}));
vi.mock('@/components/ui/EChart', () => ({
  EChart: () => <div data-testid="chart" />,
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('AmasMetricsPage', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/AmasMetricsPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders compact header, default 7d segment and four tabs', async () => {
    await renderPage();
    // 页头标题 + 副标题
    expect(screen.getByRole('heading', { name: 'AMAS 指标' })).toBeInTheDocument();
    // 默认 7d 时间窗激活
    const seg7d = screen.getByRole('button', { name: '7d' });
    expect(seg7d).toHaveAttribute('aria-pressed', 'true');
    // 导出按钮
    expect(screen.getByRole('button', { name: /导出/ })).toBeInTheDocument();
    // 四个 tab
    expect(screen.getByRole('tab', { name: '算法延迟 / 错误率' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '版本对比（预测 / 留存）' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '异常 / 不变量' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '用户状态分布' })).toBeInTheDocument();
  });

  it('shows metrics panel by default', async () => {
    await renderPage();
    // metrics 面板默认渲染：底部双轴时序卡标题 + KPI 卡片
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: '算法延迟 / 错误率' })).toBeInTheDocument(),
    );
    expect(screen.getByText('算法决策分布 · 6 路由算法')).toBeInTheDocument();
  });

  it('switches to anomalies tab', async () => {
    await renderPage();
    fireEvent.click(screen.getByRole('tab', { name: '异常 / 不变量' }));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: '异常 / 不变量违反' })).toBeInTheDocument(),
    );
    // 分级卡文案
    expect(screen.getByText('error 级')).toBeInTheDocument();
    expect(screen.getByText('不变量通过')).toBeInTheDocument();
  });

  it('switches to user-state tab', async () => {
    await renderPage();
    fireEvent.click(screen.getByRole('tab', { name: '用户状态分布' }));
    await waitFor(() =>
      expect(screen.getByRole('heading', { name: '每用户决策数分布' })).toBeInTheDocument(),
    );
    expect(screen.getByRole('heading', { name: '学习风格聚类 (k=4)' })).toBeInTheDocument();
  });

  it('switches to compare tab', async () => {
    await renderPage();
    fireEvent.click(screen.getByRole('tab', { name: '版本对比（预测 / 留存）' }));
    // versions < 2 → 空态提示
    await waitFor(() => expect(screen.getByText('至少需要两个版本')).toBeInTheDocument());
  });
});
