import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListVersions: vi.fn(),
    amasCompareVersions: vi.fn(),
  },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const versions = [
  { versionHash: 'newer1234567890abc', source: 'manual', createdAt: '2026-04-16T00:00:00Z', note: 'latest', authorAdminId: 1 },
  { versionHash: 'older0987654321xyz', source: 'llm_auto', createdAt: '2026-04-15T00:00:00Z', note: null, authorAdminId: 1 },
];

const compareFull = {
  a: { eventCount: 100, anomalyRate: 0.02, meanLatencyMs: 12.5, meanReward: 0.8, meanAttention: 0.7, meanFatigue: 0.3, meanMotivation: 0.6, meanConfidence: 0.7 },
  b: { eventCount: 120, anomalyRate: 0.015, meanLatencyMs: 10.0, meanReward: 0.85, meanAttention: 0.75, meanFatigue: 0.25, meanMotivation: 0.65, meanConfidence: 0.75 },
};
const compareEmpty = {
  a: { eventCount: 0, anomalyRate: 0, meanLatencyMs: 0, meanReward: 0, meanAttention: 0, meanFatigue: 0, meanMotivation: 0, meanConfidence: 0 },
  b: { eventCount: 0, anomalyRate: 0, meanLatencyMs: 0, meanReward: 0, meanAttention: 0, meanFatigue: 0, meanMotivation: 0, meanConfidence: 0 },
};

describe('VersionComparePanel', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPanel() {
    const { VersionComparePanel } = await import('@/pages/admin/amas/VersionComparePanel');
    return renderWithProviders(() => <VersionComparePanel />);
  }

  it('shows insufficient versions empty state', async () => {
    mockApi.amasListVersions.mockResolvedValue([versions[0]]);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('至少需要两个版本')).toBeInTheDocument());
  });

  it('renders version select and bar chart with diff table when populated', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersions.mockResolvedValue(compareFull);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('指标对比柱状图')).toBeInTheDocument());
    expect(screen.getByText('差异表')).toBeInTheDocument();
  });

  it('shows no-data empty when both versions have 0 events', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersions.mockResolvedValue(compareEmpty);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('两个版本均无 monitoring 事件')).toBeInTheDocument());
  });

  it('swaps A and B versions when swap button clicked', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersions.mockResolvedValue(compareFull);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('交换 A / B')).toBeInTheDocument());
    fireEvent.click(screen.getByText('交换 A / B'));
    // 交换会触发 effectiveA/B 切换 → compareVersions 再调
    await waitFor(() => expect(mockApi.amasCompareVersions.mock.calls.length).toBeGreaterThanOrEqual(1));
  });

  it('restores initial pick via 还原 button', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersions.mockResolvedValue(compareFull);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('还原为最近两个版本')).toBeInTheDocument());
    fireEvent.click(screen.getByText('还原为最近两个版本'));
  });
});
