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
const toast = { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() };
vi.mock('@/stores/ui', () => ({ uiStore: { toast } }));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockKpi = {
  generatedAt: '', days: 7, rangeStart: '2026-05-22', rangeEnd: '2026-05-29',
  newRegistrations: { value: 191, prevValue: 170, deltaPct: 0.124 },
  dauAverage: { value: 487, prevValue: 472, deltaPct: 0.031 },
  d7Retention: { value: 0.624, prevValue: 0.606, deltaPt: 0.018 },
  studyDurationSecs: { value: 1543860, prevValue: 1427000, deltaPct: 0.082 },
};
const mockFunnel = {
  generatedAt: '', days: 7, rangeStart: '2026-05-22', rangeEnd: '2026-05-29',
  steps: [{ key: 'register', label: '注册', sublabel: '邮箱验证完成', count: 3182, pct: 1, deltaPt: null, tone: 'good' as const }],
  biggestDropFrom: '首会话', biggestDropTo: 'd1 回访', biggestDropPt: 0.129,
};
const mockMatrix = { generatedAt: '', weeks: 7, cohorts: [{ cohortStart: '2025-10-07', size: 412, cells: [1, 0.63, 0.48, 0.41, 0.34, 0.29, 0.26] }] };
const mockDist = { generatedAt: '', days: 7, totalRecords: 184329, questionTypes: [{ key: 'w2m', label: '单词 → 释义', count: 77418, pct: 0.42 }], difficultyBins: [{ label: '≤ 1000', min: null, max: 1000, count: 12903, pct: 0.07 }] };
const mockHourly = { generatedAt: '', days: 7, matrix: Array.from({ length: 7 }, () => Array.from({ length: 24 }, (_, c) => c)), total: 184329 };
const mockWords = { generatedAt: '', days: 7, limit: 10, sort: 'count', rows: [{ rank: 1, wordId: 'w1', spelling: 'tenacious', pos: 'adj.', recordCount: 3142, accuracy: 0.62, elo: 1584 }] };
const mockInsights = { generatedAt: '', days: 7, items: [{ tone: 'info' as const, title: '周末 14:00 新峰', body: '连续 3 周。' }] };

function primeAll() {
  mockApi.analyticsKpiSummary.mockResolvedValue(mockKpi);
  mockApi.analyticsFunnel.mockResolvedValue(mockFunnel);
  mockApi.analyticsRetentionMatrix.mockResolvedValue(mockMatrix);
  mockApi.analyticsQuestionDistribution.mockResolvedValue(mockDist);
  mockApi.analyticsHourly.mockResolvedValue(mockHourly);
  mockApi.analyticsWordFrequency.mockResolvedValue(mockWords);
  mockApi.analyticsInsights.mockResolvedValue(mockInsights);
}

async function renderPage() {
  const { default: Page } = await import('@/pages/AnalyticsPage');
  return renderWithProviders(() => <Page />);
}

describe('AnalyticsPage — 自定义区间 / 导出 / 占位', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders KPI placeholder while kpi promise pending', async () => {
    primeAll();
    mockApi.analyticsKpiSummary.mockReturnValue(new Promise(() => {}));
    await renderPage();
    await waitFor(() => expect(screen.getByText('数据分析')).toBeInTheDocument());
    // 占位 4 格
    expect(screen.getAllByText('加载中').length).toBeGreaterThan(0);
  });

  it('opens custom range popover and applies from/to which drives fetch', async () => {
    primeAll();
    await renderPage();
    await waitFor(() => expect(screen.getByText('数据分析')).toBeInTheDocument());

    const rangeBtn = screen.getByText('自定义区间');
    fireEvent.click(rangeBtn);
    await waitFor(() => expect(screen.getByLabelText('起 (from)')).toBeInTheDocument());

    fireEvent.input(screen.getByLabelText('起 (from)'), { target: { value: '2026-05-01' } });
    fireEvent.input(screen.getByLabelText('止 (to)'), { target: { value: '2026-05-20' } });
    fireEvent.click(screen.getByText('应用'));

    await waitFor(() =>
      expect(mockApi.analyticsFunnel).toHaveBeenCalledWith({ from: '2026-05-01', to: '2026-05-20' }),
    );
  });

  it('export CSV builds rows and toasts success', async () => {
    primeAll();
    // jsdom 缺 createObjectURL
    const origCreate = URL.createObjectURL;
    const origRevoke = URL.revokeObjectURL;
    URL.createObjectURL = vi.fn(() => 'blob:mock');
    URL.revokeObjectURL = vi.fn();
    await renderPage();
    await waitFor(() => expect(screen.getByText('tenacious')).toBeInTheDocument());

    fireEvent.click(screen.getByText('导出全部 CSV'));
    await waitFor(() => expect(toast.success).toHaveBeenCalled());

    URL.createObjectURL = origCreate;
    URL.revokeObjectURL = origRevoke;
  });

  it('switches days segmented to 14 and refetches', async () => {
    primeAll();
    await renderPage();
    await waitFor(() => expect(screen.getByText('数据分析')).toBeInTheDocument());
    const btn14 = screen.getAllByRole('button').find((b) => b.textContent === '14 天');
    expect(btn14).toBeDefined();
    fireEvent.click(btn14!);
    await waitFor(() => expect(mockApi.analyticsQuestionDistribution).toHaveBeenCalledWith({ days: 14 }));
  });
});
