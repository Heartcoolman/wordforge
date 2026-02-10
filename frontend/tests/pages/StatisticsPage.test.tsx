import { describe, it, expect, vi, beforeEach } from 'vitest';
import { waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// Mock API modules
vi.mock('@/api/users', () => ({
  usersApi: {
    getStats: vi.fn(),
    getMe: vi.fn(),
    updateMe: vi.fn(),
    changePassword: vi.fn(),
  },
}));

vi.mock('@/api/records', () => ({
  recordsApi: {
    enhancedStatistics: vi.fn(),
    list: vi.fn(),
    create: vi.fn(),
    statistics: vi.fn(),
  },
}));

vi.mock('@/api/amas', () => ({
  amasApi: {
    getState: vi.fn(),
    getStrategy: vi.fn(),
    getPhase: vi.fn(),
    reset: vi.fn(),
  },
}));

vi.mock('@/api/wordStates', () => ({
  wordStatesApi: {
    getOverview: vi.fn(),
    get: vi.fn(),
    getDueList: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: {
    toast: {
      success: vi.fn(),
      error: vi.fn(),
      warning: vi.fn(),
      info: vi.fn(),
    },
  },
}));

import { usersApi } from '@/api/users';
import { recordsApi } from '@/api/records';
import { amasApi } from '@/api/amas';
import { wordStatesApi } from '@/api/wordStates';

const mockUsersApi = usersApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockRecordsApi = recordsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmasApi = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockWordStatesApi = wordStatesApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const fakeStats = {
  totalWordsLearned: 150,
  totalRecords: 500,
  streakDays: 7,
  accuracyRate: 0.85,
  totalSessions: 30,
};

const fakeWordOverview = {
  new: 10,
  learning: 20,
  reviewing: 15,
  mastered: 55,
};

const fakeAmasState = {
  attention: 0.8,
  fatigue: 0.3,
  motivation: 0.9,
  confidence: 0.7,
  totalEventCount: 100,
  sessionEventCount: 10,
};

function setupAllApisResolved() {
  mockUsersApi.getStats.mockResolvedValue(fakeStats);
  mockRecordsApi.enhancedStatistics.mockResolvedValue({ daily: [] });
  mockAmasApi.getState.mockResolvedValue(fakeAmasState);
  mockWordStatesApi.getOverview.mockResolvedValue(fakeWordOverview);
}

describe('StatisticsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows "学习统计" heading', async () => {
    setupAllApisResolved();
    const { default: StatisticsPage } = await import('@/pages/StatisticsPage');
    const { getByText } = renderWithProviders(() => <StatisticsPage />);
    expect(getByText('学习统计')).toBeInTheDocument();
  });

  it('shows loading spinner initially', async () => {
    // Never resolve so loading stays true
    mockUsersApi.getStats.mockReturnValue(new Promise(() => {}));
    mockRecordsApi.enhancedStatistics.mockReturnValue(new Promise(() => {}));
    mockAmasApi.getState.mockReturnValue(new Promise(() => {}));
    mockWordStatesApi.getOverview.mockReturnValue(new Promise(() => {}));

    const { default: StatisticsPage } = await import('@/pages/StatisticsPage');
    const { container } = renderWithProviders(() => <StatisticsPage />);
    const spinner = container.querySelector('[class*="animate-spin"], [role="status"]');
    expect(spinner).toBeTruthy();
  });

  it('shows 4 stat cards after data loads', async () => {
    setupAllApisResolved();
    const { default: StatisticsPage } = await import('@/pages/StatisticsPage');
    const { findByText } = renderWithProviders(() => <StatisticsPage />);

    expect(await findByText('学习单词')).toBeInTheDocument();
    expect(await findByText('总记录数')).toBeInTheDocument();
    expect(await findByText('连续天数')).toBeInTheDocument();
    expect(await findByText('正确率')).toBeInTheDocument();
  });

  it('shows stat values formatted correctly', async () => {
    setupAllApisResolved();
    const { default: StatisticsPage } = await import('@/pages/StatisticsPage');
    const { findByText } = renderWithProviders(() => <StatisticsPage />);

    // formatNumber(150) = "150", formatPercent(0.85) = "85.0%"
    expect(await findByText('150')).toBeInTheDocument();
    expect(await findByText('500')).toBeInTheDocument();
    expect(await findByText('7 天')).toBeInTheDocument();
    expect(await findByText('85.0%')).toBeInTheDocument();
  });

  it('shows "单词状态分布" section', async () => {
    setupAllApisResolved();
    const { default: StatisticsPage } = await import('@/pages/StatisticsPage');
    const { findByText } = renderWithProviders(() => <StatisticsPage />);

    expect(await findByText('单词状态分布')).toBeInTheDocument();
    expect(await findByText('新单词')).toBeInTheDocument();
    expect(await findByText('学习中')).toBeInTheDocument();
    expect(await findByText('复习中')).toBeInTheDocument();
    expect(await findByText('已掌握')).toBeInTheDocument();
  });

  it('shows word state values', async () => {
    setupAllApisResolved();
    const { default: StatisticsPage } = await import('@/pages/StatisticsPage');
    const { findByText } = renderWithProviders(() => <StatisticsPage />);

    // Wait for load
    await findByText('单词状态分布');
    expect(await findByText('10')).toBeInTheDocument();
    expect(await findByText('20')).toBeInTheDocument();
    expect(await findByText('15')).toBeInTheDocument();
    expect(await findByText('55')).toBeInTheDocument();
  });

  it('shows "认知状态" section', async () => {
    setupAllApisResolved();
    const { default: StatisticsPage } = await import('@/pages/StatisticsPage');
    const { findByText } = renderWithProviders(() => <StatisticsPage />);

    expect(await findByText('认知状态')).toBeInTheDocument();
    expect(await findByText('注意力')).toBeInTheDocument();
    expect(await findByText('疲劳度')).toBeInTheDocument();
    expect(await findByText('动机')).toBeInTheDocument();
    expect(await findByText('信心')).toBeInTheDocument();
  });
});
