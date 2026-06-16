import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '../helpers/render';

// 重构后的 AmasConfigPage 在 onMount 直接调 amasApi.getConfig()，
// 版本历史 / 灰度走 adminApi.amasListVersions() / amasGetCanary()（createResource）。
vi.mock('@/api/amas', () => ({
  amasApi: {
    getConfig: vi.fn(),
  },
}));

vi.mock('@/api/admin', () => ({
  adminApi: {
    // createResource 在模块初始化即触发，必须给安全默认值
    amasListVersions: vi.fn().mockResolvedValue([]),
    amasGetCanary: vi.fn().mockResolvedValue({ canary: null }),
    // header / 各交互按需调用
    reloadAmas: vi.fn(),
    amasUpdateConfigWithNote: vi.fn(),
    amasRestoreVersion: vi.fn(),
    amasGetVersion: vi.fn().mockResolvedValue({ snapshotJson: {} }),
    amasExplainParam: vi.fn(),
    amasConfigDiffImpact: vi.fn(),
    // TomlEditor.onMount → amasSerializeToml；解析回写 → amasParseToml
    amasSerializeToml: vi.fn().mockResolvedValue({ toml: '' }),
    amasParseToml: vi.fn(),
    // 灰度面板
    amasSetCanaryExt: vi.fn(),
    amasDisableCanary: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { amasApi } from '@/api/amas';
import { adminApi } from '@/api/admin';

const mockAmasApi = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

// 仿真后端 AMASConfig：字段名对齐重构页读取的 section.key（src/amas/config.rs）。
const mockConfig = {
  featureFlags: {
    ensembleEnabled: true,
    heuristicEnabled: true,
    igeEnabled: true,
    swdEnabled: true,
    mdmEnabled: true,
    iadEnabled: false,
    mtpEnabled: false,
    sspEnabled: false,
  },
  coldStart: {
    classifyToExploreEvents: 5,
    exploreToExploitEvents: 20,
    classifyToExploreConfidence: 0.6,
  },
  classifier: { fastLearnerThreshold: 0.8, stableLearnerThreshold: 0.45 },
  constraints: {
    highFatigueThreshold: 0.6,
    maxBatchSizeWhenFatigued: 10,
    maxDifficultyWhenFatigued: 5,
    maxNewRatioWhenFatigued: 0.3,
  },
  intervention: {
    fatigueAlertThreshold: 0.7,
    attentionAlertThreshold: 0.3,
    motivationAlertThreshold: -0.3,
  },
  elo: {
    defaultElo: 1200,
    kFactor: 24,
    noviceKMultiplier: 2,
    zpdOptimalOffset: 0,
    zpdGaussianSigma: 200,
  },
  ensemble: {
    baseWeightHeuristic: 0.4,
    baseWeightIge: 0.3,
    baseWeightSwd: 0.3,
    minWeight: 0.15,
    warmupSamples: 20,
  },
};

describe('AmasConfigPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // clearAllMocks 会清掉 createResource 用的默认值，重新打底
    mockAdminApi.amasListVersions.mockResolvedValue([]);
    mockAdminApi.amasGetCanary.mockResolvedValue({ canary: null });
    mockAdminApi.amasSerializeToml.mockResolvedValue({ toml: '' });
  });

  async function renderPage() {
    const { default: AmasConfigPage } = await import('@/pages/AmasConfigPage');
    return renderWithProviders(() => <AmasConfigPage />);
  }

  it('renders page title after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('AMAS 调参')).toBeInTheDocument();
    });
  });

  it('shows loading spinner initially', async () => {
    // getConfig 永不 resolve → 停在 <Loading/>（wf Spinner: <div class="spinner">）
    mockAmasApi.getConfig.mockReturnValue(new Promise(() => {}));
    const { container } = await renderPage();
    expect(container.querySelector('.spinner')).toBeInTheDocument();
  });

  it('shows four tabs after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    const { container } = await renderPage();
    await waitFor(() => {
      expect(container.querySelector('.tabs')).toBeInTheDocument();
    });
    // 重构后 tab 集合：可视化配置 / 高级 (TOML) / 版本历史 / 灰度发布。
    // 「版本历史」header 与 tab 同名，故按 .tabs 容器内文本断言以消歧。
    const tabBar = container.querySelector('.tabs') as HTMLElement;
    const tabLabels = Array.from(tabBar.querySelectorAll('.tab')).map((b) => b.textContent ?? '');
    expect(tabLabels.some((t) => t.includes('可视化配置'))).toBe(true);
    expect(tabLabels.some((t) => t.includes('高级 (TOML)'))).toBe(true);
    expect(tabLabels.some((t) => t.includes('版本历史'))).toBe(true);
    expect(tabLabels.some((t) => t.includes('灰度发布'))).toBe(true);
  });

  it('shows 保存为新版本 button after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    await renderPage();
    await waitFor(() => {
      // header 重构后主操作：保存为新版本（dirty 时才可用，存在性始终满足）
      expect(screen.getByRole('button', { name: '保存为新版本' })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: '热重载' })).toBeInTheDocument();
    });
  });

  it('renders config group sections on visual tab', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    await renderPage();
    await waitFor(() => {
      // 可视化配置默认 tab：分组卡片标题
      expect(screen.getByText('一键预设')).toBeInTheDocument();
      expect(screen.getByText('新手探索')).toBeInTheDocument();
      expect(screen.getByText('算法配比')).toBeInTheDocument();
      expect(screen.getByText('功能开关')).toBeInTheDocument();
    });
  });

  it('renders tunable param labels on visual tab', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    await renderPage();
    await waitFor(() => {
      // 具体可调参数 label（新手探索 / 水平评分 ELO 分组下）
      expect(screen.getByText('画像所需事件数')).toBeInTheDocument();
      expect(screen.getByText('稳定所需事件数')).toBeInTheDocument();
      expect(screen.getByText('评分灵敏度 K')).toBeInTheDocument();
    });
  });

  it('applying a preset marks config dirty (改动预览 populated)', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    const user = userEvent.setup();
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('激进')).toBeInTheDocument();
    });
    // 「激进」预设的 patch 会改动 exploreToExploitEvents/kFactor/highFatigueThreshold
    await user.click(screen.getByText('激进'));
    await waitFor(() => {
      // dirty 后 header 出现「有未保存修改」徽标，且改动预览不再是空态
      expect(screen.getAllByText('有未保存修改').length).toBeGreaterThan(0);
      expect(screen.getByText('稳定所需事件数')).toBeInTheDocument();
    });
  });

  it('clicking 评估 calls amasConfigDiffImpact', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAdminApi.amasConfigDiffImpact.mockResolvedValue({
      telemetrySampleSize: 1200,
      confidence: 'medium',
      fields: [],
    });
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '评估' })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: '评估' }));
    await waitFor(() => {
      expect(mockAdminApi.amasConfigDiffImpact).toHaveBeenCalled();
    });
  });

  it('switching to 高级 (TOML) tab serializes config via amasSerializeToml', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAdminApi.amasSerializeToml.mockResolvedValue({
      toml: '[memoryModel]\nbaseDesiredRetention = 0.92\n',
    });
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: /高级 \(TOML\)/ })).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole('button', { name: /高级 \(TOML\)/ }));
    await waitFor(() => {
      // TOML 视图卡片标题 + onMount 触发的序列化调用
      expect(screen.getByText('配置源 (TOML)')).toBeInTheDocument();
      expect(mockAdminApi.amasSerializeToml).toHaveBeenCalled();
    });
  });
});
