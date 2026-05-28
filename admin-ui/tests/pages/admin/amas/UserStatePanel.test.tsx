import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: { amasUserStateDistribution: vi.fn() },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const emptyDist = {
  attention: { field: 'attention', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
  fatigue: { field: 'fatigue', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
  motivation: { field: 'motivation', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
  confidence: { field: 'confidence', min: 0, max: 1, bins: [], mean: 0, median: 0, sampleSize: 0 },
  coldStartExplore: 0,
  coldStartExploit: 0,
};
const fullDist = {
  attention: { field: 'attention', min: 0, max: 1, bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], mean: 0.5, median: 0.5, sampleSize: 100 },
  fatigue: { field: 'fatigue', min: 0, max: 1, bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], mean: 0.3, median: 0.3, sampleSize: 100 },
  motivation: { field: 'motivation', min: -1, max: 1, bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], mean: 0.6, median: 0.6, sampleSize: 100 },
  confidence: { field: 'confidence', min: 0, max: 1, bins: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20], mean: 0.7, median: 0.7, sampleSize: 100 },
  coldStartExplore: 5,
  coldStartExploit: 8,
};

describe('UserStatePanel', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPanel() {
    const { UserStatePanel } = await import('@/pages/admin/amas/UserStatePanel');
    return renderWithProviders(() => <UserStatePanel />);
  }

  it('renders heading and pickers', async () => {
    mockApi.amasUserStateDistribution.mockResolvedValue(emptyDist);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('用户状态分布')).toBeInTheDocument());
    expect(screen.getByText('窗口')).toBeInTheDocument();
    expect(screen.getByText('桶数')).toBeInTheDocument();
  });

  it('shows empty state for zero samples', async () => {
    mockApi.amasUserStateDistribution.mockResolvedValue(emptyDist);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('窗口内无采样')).toBeInTheDocument());
  });

  it('renders histograms for all 4 fields when data present', async () => {
    mockApi.amasUserStateDistribution.mockResolvedValue(fullDist);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('注意力')).toBeInTheDocument());
    expect(screen.getByText('疲劳')).toBeInTheDocument();
    expect(screen.getByText('动机')).toBeInTheDocument();
    expect(screen.getByText('信心')).toBeInTheDocument();
    expect(screen.getByText(/冷启动样本/)).toBeInTheDocument();
  });

  it('changes window and bins via buttons', async () => {
    mockApi.amasUserStateDistribution.mockResolvedValue(emptyDist);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('7天')).toBeInTheDocument());
    fireEvent.click(screen.getByText('7天'));
    await waitFor(() => expect(mockApi.amasUserStateDistribution).toHaveBeenCalledWith(7, expect.any(Number)));
    fireEvent.click(screen.getByText('40'));
    await waitFor(() => expect(mockApi.amasUserStateDistribution).toHaveBeenCalledWith(expect.any(Number), 40));
  });
});
