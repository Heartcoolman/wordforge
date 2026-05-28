import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/amas', () => ({
  amasApi: {
    getConfig: vi.fn(),
    updateConfig: vi.fn(),
    getMetrics: vi.fn(),
  },
}));

vi.mock('@/api/admin', () => ({
  adminApi: {
    reloadAmas: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { amasApi } from '@/api/amas';

const mockAmasApi = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

// 仿真后端返回的 AMASConfig：仅含 Tier-A 用到的字段，其余字段省略（schema.validateConfig 对缺失字段静默放行）
const mockConfig = {
  featureFlags: { ensembleEnabled: true, heuristicEnabled: true, igeEnabled: true, swdEnabled: true, mdmEnabled: true, iadEnabled: false, mtpEnabled: false, sspEnabled: false },
  memoryModel: {
    w: [0.4072, 1.1829, 3.1262, 15.4722, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192, 1.01925, 1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621],
    baseDesiredRetention: 0.92,
    maxIntervalDays: 90,
  },
  ensemble: { baseWeightHeuristic: 0.4, baseWeightIge: 0.3, baseWeightSwd: 0.3, warmupSamples: 20, blendScale: 100, blendMax: 0.5, minWeight: 0.15, warmupHeuristicBoost: 0.2 },
  objectiveWeights: { retention: 0.35, accuracy: 0.25, speed: 0.15, fatigue: 0.15, frustration: 0.1 },
};
const mockMetrics = {
  Heuristic: { callCount: 100, totalLatencyUs: 200000, errorCount: 0 },
};

describe('AmasConfigPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: AmasConfigPage } = await import('@/pages/admin/AmasConfigPage');
    return renderWithProviders(() => <AmasConfigPage />);
  }

  it('renders page title after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('AMAS 调参')).toBeInTheDocument();
    });
  });

  it('shows loading spinner initially', async () => {
    mockAmasApi.getConfig.mockReturnValue(new Promise(() => {}));
    mockAmasApi.getMetrics.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows three tabs after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /重点参数/ })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /分节配置/ })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: /JSON 高级/ })).toBeInTheDocument();
    });
  });

  it('shows save button after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '保存配置' })).toBeInTheDocument();
    });
  });

  it('shows 算法指标 section after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('算法指标')).toBeInTheDocument();
    });
  });

  it('renders Tier-A params on first tab', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('目标长期留存率')).toBeInTheDocument();
      expect(screen.getByText('最大调度间隔（天）')).toBeInTheDocument();
    });
  });

  it('switching to JSON 高级 tab reveals textarea with config', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('tab', { name: /JSON 高级/ })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('tab', { name: /JSON 高级/ }));
    await waitFor(() => {
      const textarea = document.querySelector('textarea');
      expect(textarea).toBeInTheDocument();
      expect(textarea!.value).toContain('memoryModel');
      expect(textarea!.value).toContain('baseDesiredRetention');
    });
  });
});
