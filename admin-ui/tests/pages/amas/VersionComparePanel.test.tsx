import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListVersions: vi.fn(),
    amasCompareVersionsExt: vi.fn(),
  },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const versions = [
  { id: 2, versionHash: 'newer1234567890abc', source: 'manual', createdAt: '2026-04-16T00:00:00Z', note: 'latest', authorAdminId: '1', parentVersionHash: 'older0987654321xyz' },
  { id: 1, versionHash: 'older0987654321xyz', source: 'llm_auto', createdAt: '2026-04-15T00:00:00Z', note: null, authorAdminId: '1', parentVersionHash: null },
];

const slice = (versionHash: string, eventCount: number) => ({
  versionHash,
  eventCount,
  hitRate: 0.62,
  p95LatencyMs: 12.5,
  meanLatencyMs: 8.1,
  fatigueRate: 0.3,
  ensembleShare: 0.55,
  meanReward: 0.8,
  anomalyRate: 0.02,
  retention7d: 0.45,
  p95CompletionMs: 4200,
  spark: [1, 2, 3, 4],
  configEpsilon: 0.12,
  firstEventAt: '2026-04-15T00:00:00Z',
  lastEventAt: '2026-04-16T00:00:00Z',
});

const compareFull = { a: slice('older0987654321xyz', 100), b: slice('newer1234567890abc', 120) };
const compareEmpty = { a: slice('older0987654321xyz', 0), b: slice('newer1234567890abc', 0) };

describe('VersionComparePanel', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPanel() {
    const { VersionComparePanel } = await import('@/pages/amas/VersionComparePanel');
    return renderWithProviders(() => <VersionComparePanel />);
  }

  it('shows insufficient versions empty state', async () => {
    mockApi.amasListVersions.mockResolvedValue([versions[0]]);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('至少需要两个版本')).toBeInTheDocument());
  });

  it('renders metric rows and advice when populated with events', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersionsExt.mockResolvedValue(compareFull);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('7d 命中率')).toBeInTheDocument());
    expect(screen.getByText('P95 决策耗时')).toBeInTheDocument();
    expect(screen.getByText('7d 留存')).toBeInTheDocument();
    expect(screen.getByText('P95 答题完成时间')).toBeInTheDocument();
    expect(screen.getByText('建议：')).toBeInTheDocument();
  });

  it('shows no-data empty when both versions have 0 events', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersionsExt.mockResolvedValue(compareEmpty);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('两个版本均无 monitoring 事件')).toBeInTheDocument());
  });
});
