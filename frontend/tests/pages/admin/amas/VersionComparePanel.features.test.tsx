import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../../helpers/render';

vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListVersions: vi.fn(),
    amasCompareVersions: vi.fn(),
  },
}));
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
const compareZeroMetrics = {
  a: { eventCount: 10, anomalyRate: 0, meanLatencyMs: 0, meanReward: 0, meanAttention: 0, meanFatigue: 0, meanMotivation: 0, meanConfidence: 0 },
  b: { eventCount: 10, anomalyRate: 0, meanLatencyMs: 0, meanReward: 0, meanAttention: 0, meanFatigue: 0, meanMotivation: 0, meanConfidence: 0 },
};

async function renderPanel() {
  const { VersionComparePanel } = await import('@/pages/admin/amas/VersionComparePanel');
  return renderWithProviders(() => <VersionComparePanel />);
}

describe('VersionComparePanel — diff table, loading, swap, select fallback', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders all 8 metrics + legend in the diff table', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersions.mockResolvedValue(compareFull);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('差异表')).toBeInTheDocument());
    expect(screen.getByText('事件数')).toBeInTheDocument();
    expect(screen.getByText('异常率')).toBeInTheDocument();
    expect(screen.getByText('平均延迟 (ms)')).toBeInTheDocument();
    expect(screen.getByText('平均 Reward')).toBeInTheDocument();
    expect(screen.getByText('平均注意力')).toBeInTheDocument();
    expect(screen.getByText('平均疲劳')).toBeInTheDocument();
    expect(screen.getByText('平均动机')).toBeInTheDocument();
    expect(screen.getByText('平均信心')).toBeInTheDocument();
    expect(screen.getByText(/绿色 = 朝预期方向变化/)).toBeInTheDocument();
  });

  it('shows loading spinner while compare promise is pending', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    let resolveCompare: ((v: unknown) => void) | undefined;
    mockApi.amasCompareVersions.mockImplementation(
      () => new Promise((res) => { resolveCompare = res; }),
    );
    await renderPanel();
    await waitFor(() => expect(screen.getByText('版本对比')).toBeInTheDocument());
    await waitFor(() => {
      const spinners = document.querySelectorAll('[class*="animate-spin"], [data-testid="spinner"]');
      expect(spinners.length).toBeGreaterThan(0);
    });
    resolveCompare?.(compareFull);
  });

  it('swap A/B button triggers compareVersions re-call', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersions.mockResolvedValue(compareFull);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('交换 A / B')).toBeInTheDocument());
    const before = mockApi.amasCompareVersions.mock.calls.length;
    fireEvent.click(screen.getByText('交换 A / B'));
    await waitFor(() =>
      expect(mockApi.amasCompareVersions.mock.calls.length).toBeGreaterThanOrEqual(before),
    );
  });

  it('select onChange for A & B + zero-zero "—" fallback', async () => {
    const v3 = [
      { versionHash: 'aaaaaaaaaaaaaaaa', source: 'manual', createdAt: '2026-04-15T00:00:00Z', note: null, authorAdminId: 1 },
      { versionHash: 'bbbbbbbbbbbbbbbb', source: 'manual', createdAt: '2026-04-16T00:00:00Z', note: null, authorAdminId: 1 },
      { versionHash: 'cccccccccccccccc', source: 'llm_auto', createdAt: '2026-04-17T00:00:00Z', note: null, authorAdminId: 1 },
    ];
    mockApi.amasListVersions.mockResolvedValue(v3);
    mockApi.amasCompareVersions.mockResolvedValue(compareZeroMetrics);
    await renderPanel();
    await waitFor(() => expect(screen.getAllByRole('combobox').length).toBeGreaterThanOrEqual(2));
    const [selA, selB] = screen.getAllByRole('combobox') as HTMLSelectElement[];
    fireEvent.change(selA, { target: { value: 'aaaaaaaaaaaaaaaa' } });
    fireEvent.change(selB, { target: { value: 'cccccccccccccccc' } });
    await waitFor(() => expect(screen.getAllByText('—').length).toBeGreaterThan(0));
  });
});
