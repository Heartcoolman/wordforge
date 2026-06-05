import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    analyticsKpiSummary: vi.fn(),
    analyticsFunnel: vi.fn(),
    analyticsRetentionMatrix: vi.fn(),
    analyticsQuestionDistribution: vi.fn(),
    analyticsHourly: vi.fn(),
    analyticsWordFrequency: vi.fn(),
    analyticsInsights: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';

const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockKpi = {
  generatedAt: '', days: 7, rangeStart: '2026-05-22', rangeEnd: '2026-05-29',
  newRegistrations: { value: 191, prevValue: 170, deltaPct: 0.124 },
  dauAverage: { value: 487, prevValue: 472, deltaPct: 0.031 },
  d7Retention: { value: 0.624, prevValue: 0.606, deltaPt: 0.018 },
  studyDurationSecs: { value: 1543860, prevValue: 1427000, deltaPct: 0.082 },
};
const mockFunnel = {
  generatedAt: '', days: 7, rangeStart: '2026-05-22', rangeEnd: '2026-05-29',
  steps: [
    { key: 'register', label: '注册', sublabel: '邮箱验证完成', count: 3182, pct: 1, deltaPt: null, tone: 'good' as const },
    { key: 'pick', label: '选择词库', sublabel: '至少选一', count: 2902, pct: 0.912, deltaPt: 0.023, tone: 'good' as const },
    { key: 'd30', label: 'd30 留存', sublabel: '稳态期', count: 703, pct: 0.221, deltaPt: 0.009, tone: 'warn' as const },
  ],
  biggestDropFrom: '首会话', biggestDropTo: 'd1 回访', biggestDropPt: 0.129,
};
const mockMatrix = {
  generatedAt: '', weeks: 7,
  cohorts: [
    { cohortStart: '2025-10-07', size: 412, cells: [1, 0.63, 0.48, 0.41, 0.34, 0.29, 0.26] },
    { cohortStart: '2025-11-18', size: 601, cells: [1, null, null, null, null, null, null] },
  ],
};
const mockDist = {
  generatedAt: '', days: 7, totalRecords: 184329,
  questionTypes: [
    { key: 'w2m', label: '单词 → 释义', count: 77418, pct: 0.42 },
    { key: 'm2w', label: '释义 → 单词', count: 51612, pct: 0.28 },
  ],
  difficultyBins: [
    { label: '≤ 1000', min: null, max: 1000, count: 12903, pct: 0.07 },
    { label: '1200–1400', min: 1200, max: 1400, count: 62672, pct: 0.34 },
  ],
};
const mockHourly = {
  generatedAt: '', days: 7,
  matrix: Array.from({ length: 7 }, (_, r) => Array.from({ length: 24 }, (_, c) => (r === 6 && c === 21 ? 692 : c))),
  total: 184329,
};
const mockWords = {
  generatedAt: '', days: 7, limit: 10, sort: 'count',
  rows: [
    { rank: 1, wordId: 'w1', spelling: 'tenacious', pos: 'adj.', recordCount: 3142, accuracy: 0.62, elo: 1584 },
    { rank: 2, wordId: 'w2', spelling: 'acquiesce', pos: 'v.', recordCount: 2612, accuracy: 0.41, elo: 1712 },
  ],
};
const mockInsights = {
  generatedAt: '', days: 7,
  items: [
    { tone: 'success' as const, title: 'd7 留存连续两周回升', body: '归因 cold_start 阶段延长。' },
    { tone: 'warning' as const, title: 'acquiesce 正确率 < 50%', body: 'ELO 评分可能偏高。' },
  ],
};

function primeMocks() {
  mockAdminApi.analyticsKpiSummary.mockResolvedValue(mockKpi);
  mockAdminApi.analyticsFunnel.mockResolvedValue(mockFunnel);
  mockAdminApi.analyticsRetentionMatrix.mockResolvedValue(mockMatrix);
  mockAdminApi.analyticsQuestionDistribution.mockResolvedValue(mockDist);
  mockAdminApi.analyticsHourly.mockResolvedValue(mockHourly);
  mockAdminApi.analyticsWordFrequency.mockResolvedValue(mockWords);
  mockAdminApi.analyticsInsights.mockResolvedValue(mockInsights);
}

function primeEmpty() {
  mockAdminApi.analyticsKpiSummary.mockResolvedValue({
    ...mockKpi,
    newRegistrations: { value: 0, prevValue: 0, deltaPct: null },
    dauAverage: { value: 0, prevValue: 0, deltaPct: null },
    d7Retention: { value: 0, prevValue: 0, deltaPt: null },
    studyDurationSecs: { value: 0, prevValue: 0, deltaPct: null },
  });
  mockAdminApi.analyticsFunnel.mockResolvedValue({ ...mockFunnel, steps: [] });
  mockAdminApi.analyticsRetentionMatrix.mockResolvedValue({ generatedAt: '', weeks: 7, cohorts: [] });
  mockAdminApi.analyticsQuestionDistribution.mockResolvedValue({
    generatedAt: '', days: 7, totalRecords: 0, questionTypes: [], difficultyBins: [],
  });
  mockAdminApi.analyticsHourly.mockResolvedValue({ generatedAt: '', days: 7, matrix: Array.from({ length: 7 }, () => Array(24).fill(0)), total: 0 });
  mockAdminApi.analyticsWordFrequency.mockResolvedValue({ generatedAt: '', days: 7, limit: 10, sort: 'count', rows: [] });
  mockAdminApi.analyticsInsights.mockResolvedValue({ generatedAt: '', days: 7, items: [] });
}

async function renderPage() {
  const { default: AnalyticsPage } = await import('@/pages/AnalyticsPage');
  return renderWithProviders(() => <AnalyticsPage />);
}

describe('AnalyticsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders page heading after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('数据分析')).toBeInTheDocument());
  });

  it('shows KPI cards after loading', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('7 天新注册')).toBeInTheDocument());
    expect(screen.getByText('7 天活跃 (DAU 平均)')).toBeInTheDocument();
    expect(screen.getByText('d7 留存率')).toBeInTheDocument();
    expect(screen.getByText('7 天累计学习时长')).toBeInTheDocument();
  });

  it('renders funnel rows with biggest drop note', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('注册到长期留存 · 漏斗')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('选择词库')).toBeInTheDocument());
    expect(screen.getByText(/最大流失点/)).toBeInTheDocument();
  });

  it('renders cohort retention matrix', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('留存矩阵 · 7 周 cohort')).toBeInTheDocument());
    // cohortStart 2025-10-07 → 显示 10-07
    await waitFor(() => expect(screen.getByText('10-07')).toBeInTheDocument());
  });

  it('renders question distribution groups', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('答题分布')).toBeInTheDocument());
    expect(screen.getByText('题型')).toBeInTheDocument();
    expect(screen.getByText('难度（ELO 分箱）')).toBeInTheDocument();
    expect(screen.getByText('单词 → 释义')).toBeInTheDocument();
  });

  it('renders heatmap with peak annotation', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('学习时段热图 · 7d × 24h')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText(/峰值/)).toBeInTheDocument());
  });

  it('renders word frequency rows', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('词频 Top')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('tenacious')).toBeInTheDocument());
    expect(screen.getByText('acquiesce')).toBeInTheDocument();
  });

  it('renders insights list', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('本期洞察')).toBeInTheDocument());
    await waitFor(() => expect(screen.getByText('d7 留存连续两周回升')).toBeInTheDocument());
  });

  it('shows empty funnel state when no steps', async () => {
    primeEmpty();
    await renderPage();
    await waitFor(() => expect(screen.getByText('本窗口暂无注册队列')).toBeInTheDocument());
  });

  it('shows empty insights state when items empty', async () => {
    primeEmpty();
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无洞察')).toBeInTheDocument());
  });

  it('updates window to 90 days via segmented and refetches', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('数据分析')).toBeInTheDocument());
    const btn90 = screen.getAllByRole('button').find((b) => b.textContent === '90 天');
    expect(btn90).toBeDefined();
    fireEvent.click(btn90!);
    await waitFor(() => expect(mockAdminApi.analyticsKpiSummary).toHaveBeenCalledWith({ days: 90 }));
  });

  it('switches word sort to accuracy and refetches', async () => {
    primeMocks();
    await renderPage();
    await waitFor(() => expect(screen.getByText('词频 Top')).toBeInTheDocument());
    const errBtn = screen.getAllByRole('button').find((b) => b.textContent === '错误率');
    expect(errBtn).toBeDefined();
    fireEvent.click(errBtn!);
    await waitFor(() =>
      expect(mockAdminApi.analyticsWordFrequency).toHaveBeenCalledWith(
        expect.objectContaining({ sort: 'accuracy', limit: 10 }),
      ),
    );
  });
});
