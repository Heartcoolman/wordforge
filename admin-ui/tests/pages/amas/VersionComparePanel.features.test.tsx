import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListVersions: vi.fn(),
    amasCompareVersionsExt: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const versions = [
  { id: 2, versionHash: 'newer1234567890abc', source: 'manual', createdAt: '2026-04-16T00:00:00Z', note: 'latest', authorAdminId: '1', parentVersionHash: null },
  { id: 1, versionHash: 'older0987654321xyz', source: 'llm_auto', createdAt: '2026-04-15T00:00:00Z', note: null, authorAdminId: '1', parentVersionHash: null },
];

function slice(versionHash: string, over: Partial<Record<string, unknown>> = {}) {
  return {
    versionHash,
    eventCount: 100,
    hitRate: 0.7,
    p95LatencyMs: 12.5,
    meanLatencyMs: 10.0,
    fatigueRate: 0.3,
    ensembleShare: 0.5,
    meanReward: 0.8,
    anomalyRate: 0.02,
    retention7d: 0.6,
    p95CompletionMs: 4200,
    spark: [1, 2, 3, 4],
    configEpsilon: 0.1,
    firstEventAt: '2026-04-15T00:00:00Z',
    lastEventAt: '2026-04-16T00:00:00Z',
    ...over,
  };
}

// B 在各项上优于 A → advice 必然命中 wins 分支
const compareFull = {
  a: slice('older0987654321xyz', { hitRate: 0.6, p95LatencyMs: 15.0, fatigueRate: 0.35, anomalyRate: 0.03 }),
  b: slice('newer1234567890abc', { hitRate: 0.7, p95LatencyMs: 12.0, fatigueRate: 0.25, anomalyRate: 0.015 }),
};

async function renderPanel() {
  const { VersionComparePanel } = await import('@/pages/amas/VersionComparePanel');
  return renderWithProviders(() => <VersionComparePanel />);
}

describe('VersionComparePanel — native selects, ext compare re-call, advice banner', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders two native selects with one option per version when ≥2 versions', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersionsExt.mockResolvedValue(compareFull);
    await renderPanel();

    await waitFor(() => expect(screen.getAllByRole('combobox').length).toBe(2));
    const [selA, selB] = screen.getAllByRole('combobox') as HTMLSelectElement[];
    expect(selA.options.length).toBe(2);
    expect(selB.options.length).toBe(2);
    // 选项 label 含 versionHash 前缀 + source
    expect(screen.getByLabelText('基线版本')).toBeInTheDocument();
    expect(screen.getByLabelText('对比版本')).toBeInTheDocument();
  });

  it('changing a select value re-calls amasCompareVersionsExt with the new versionHash', async () => {
    const v3 = [
      { id: 3, versionHash: 'cccccccccccccccc', source: 'llm_auto', createdAt: '2026-04-17T00:00:00Z', note: null, authorAdminId: '1', parentVersionHash: null },
      { id: 2, versionHash: 'bbbbbbbbbbbbbbbb', source: 'manual', createdAt: '2026-04-16T00:00:00Z', note: null, authorAdminId: '1', parentVersionHash: null },
      { id: 1, versionHash: 'aaaaaaaaaaaaaaaa', source: 'manual', createdAt: '2026-04-15T00:00:00Z', note: null, authorAdminId: '1', parentVersionHash: null },
    ];
    mockApi.amasListVersions.mockResolvedValue(v3);
    mockApi.amasCompareVersionsExt.mockImplementation((a: string, b: string) =>
      Promise.resolve({ a: slice(a), b: slice(b) }),
    );
    await renderPanel();

    await waitFor(() => expect(screen.getAllByRole('combobox').length).toBe(2));
    const [selA] = screen.getAllByRole('combobox') as HTMLSelectElement[];

    fireEvent.change(selA, { target: { value: 'aaaaaaaaaaaaaaaa' } });

    await waitFor(() =>
      expect(
        mockApi.amasCompareVersionsExt.mock.calls.some((c) => c[0] === 'aaaaaaaaaaaaaaaa'),
      ).toBe(true),
    );
  });

  it('shows the "建议：" advice banner once compare data is loaded', async () => {
    mockApi.amasListVersions.mockResolvedValue(versions);
    mockApi.amasCompareVersionsExt.mockResolvedValue(compareFull);
    await renderPanel();

    await waitFor(() => expect(screen.getByText('建议：')).toBeInTheDocument());
    // B 优于 A → 命中 wins 分支文案
    expect(screen.getByText(/B 版本在/)).toBeInTheDocument();
  });
});
