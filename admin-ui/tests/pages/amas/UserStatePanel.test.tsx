import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasStageDistribution: vi.fn(),
    amasDecisionHistogram: vi.fn(),
    amasStateTransitions: vi.fn(),
    amasLearningClusters: vi.fn(),
  },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const emptyStage = { totalUsers: 0, stages: [], trend: [] };
const fullStage = {
  totalUsers: 320,
  stages: [
    { stage: 'cold', users: 100, pct: 0.3125, avgDecisions: 5.2, retention7d: 0.6, mainRoute: 'mdm' },
    { stage: 'transition', users: 120, pct: 0.375, avgDecisions: 80.1, retention7d: 0.7, mainRoute: 'swd' },
    { stage: 'stable', users: 100, pct: 0.3125, avgDecisions: 340.5, retention7d: 0.85, mainRoute: 'ensemble' },
  ],
  trend: [],
};
const fullHist = {
  buckets: [
    { label: '0-15', count: 40 },
    { label: '15-50', count: 60 },
    { label: '50-200', count: 80 },
  ],
  p50: 45,
  p95: 210,
  totalUsers: 180,
};
const emptyHist = { buckets: [], p50: 0, p95: 0, totalUsers: 0 };
const fullFlow = {
  windowHours: 24,
  transitions: [
    { from: 'cold', to: 'transition', count: 12 },
    { from: 'transition', to: 'stable', count: 5 },
  ],
};
const emptyFlow = { windowHours: 24, transitions: [] };
const fullClusters = {
  k: 4,
  totalUsers: 200,
  clusters: [
    { label: '快速低错', count: 60, pct: 0.3, avgResponseMs: 1200, errorRate: 0.1, recordsPerActiveDay: 30 },
    { label: '慢速高错', count: 40, pct: 0.2, avgResponseMs: 4500, errorRate: 0.4, recordsPerActiveDay: 8 },
  ],
};
const emptyClusters = { k: 4, totalUsers: 0, clusters: [] };

describe('UserStatePanel', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPanel() {
    const { UserStatePanel } = await import('@/pages/amas/UserStatePanel');
    return renderWithProviders(() => <UserStatePanel />);
  }

  it('renders 3 stage cards and 4 section titles when populated', async () => {
    mockApi.amasStageDistribution.mockResolvedValue(fullStage);
    mockApi.amasDecisionHistogram.mockResolvedValue(fullHist);
    mockApi.amasStateTransitions.mockResolvedValue(fullFlow);
    mockApi.amasLearningClusters.mockResolvedValue(fullClusters);
    await renderPanel();

    await waitFor(() => expect(screen.getByText('冷启动')).toBeInTheDocument());
    expect(screen.getByText('过渡期')).toBeInTheDocument();
    expect(screen.getByText('稳定阶段')).toBeInTheDocument();
    expect(screen.getByText('每用户决策数分布')).toBeInTheDocument();
    expect(screen.getByText('状态切换流向 (24h)')).toBeInTheDocument();
    expect(screen.getByText('学习风格聚类 (k=4)')).toBeInTheDocument();
  });

  it('shows empty state when stage totalUsers is 0', async () => {
    mockApi.amasStageDistribution.mockResolvedValue(emptyStage);
    mockApi.amasDecisionHistogram.mockResolvedValue(emptyHist);
    mockApi.amasStateTransitions.mockResolvedValue(emptyFlow);
    mockApi.amasLearningClusters.mockResolvedValue(emptyClusters);
    await renderPanel();

    await waitFor(() => expect(screen.getByText('暂无用户状态')).toBeInTheDocument());
    expect(screen.queryByText('冷启动')).not.toBeInTheDocument();
  });

  it('calls all four amas endpoints', async () => {
    mockApi.amasStageDistribution.mockResolvedValue(fullStage);
    mockApi.amasDecisionHistogram.mockResolvedValue(fullHist);
    mockApi.amasStateTransitions.mockResolvedValue(fullFlow);
    mockApi.amasLearningClusters.mockResolvedValue(fullClusters);
    await renderPanel();

    await waitFor(() => expect(mockApi.amasStageDistribution).toHaveBeenCalled());
    expect(mockApi.amasDecisionHistogram).toHaveBeenCalledWith(7);
    expect(mockApi.amasStateTransitions).toHaveBeenCalledWith(24);
    expect(mockApi.amasLearningClusters).toHaveBeenCalled();
  });
});
