import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/amas', () => ({
  amasApi: {
    getConfig: vi.fn(),
  },
}));
vi.mock('@/api/admin', () => ({
  adminApi: {
    reloadAmas: vi.fn(),
    amasUpdateConfigWithNote: vi.fn(),
    amasListVersions: vi.fn(() => Promise.resolve([])),
    amasGetVersion: vi.fn(),
    amasRestoreVersion: vi.fn(),
    amasGetCanary: vi.fn(() => Promise.resolve({ canary: null })),
    amasSetCanaryExt: vi.fn(),
    amasDisableCanary: vi.fn(),
    amasParseToml: vi.fn(),
    amasSerializeToml: vi.fn(() => Promise.resolve({ toml: '# stub\n' })),
    amasConfigDiffImpact: vi.fn(),
    amasExplainParam: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { amasApi } from '@/api/amas';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockAmas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

// 与新版页面分节字段对齐（src/amas/config.rs → AMASConfig），仅含页面读取的 section。
const baseConfig = {
  coldStart: {
    classifyToExploreEvents: 6,
    exploreToExploitEvents: 20,
    classifyToExploreConfidence: 0.6,
  },
  classifier: { fastLearnerThreshold: 0.8, stableLearnerThreshold: 0.5 },
  constraints: {
    highFatigueThreshold: 0.6,
    maxBatchSizeWhenFatigued: 10,
    maxDifficultyWhenFatigued: 5,
    maxNewRatioWhenFatigued: 0.4,
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
    minWeight: 0.05,
    warmupSamples: 50,
  },
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
};

function freshConfig() {
  return structuredClone(baseConfig);
}

async function renderPage() {
  const { default: Page } = await import('@/pages/AmasConfigPage');
  return renderWithProviders(() => <Page />);
}

/** 拖动第一个滑块（新手探索 → 画像所需事件数，min 1 / max 20），使页面进入 dirty 状态 */
function makeDirty() {
  const range = document.querySelector('input[type="range"]') as HTMLInputElement;
  // baseline 该项为 6，改成 12 触发 dirty
  fireEvent.input(range, { target: { value: '12' } });
}

describe('AmasConfigPage — CRUD & error fallbacks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAdmin.amasSerializeToml.mockResolvedValue({ toml: '# stub\n' });
    mockAdmin.amasListVersions.mockResolvedValue([]);
    mockAdmin.amasGetCanary.mockResolvedValue({ canary: null });
  });

  it('handles getConfig failure with toast', async () => {
    mockAmas.getConfig.mockRejectedValue(new Error('boom'));
    await renderPage();
    // onMount catch → toast.error('加载失败', msg) + 错误回退卡片
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('加载失败', 'boom'));
    expect(screen.getByText('加载失败，请重试')).toBeInTheDocument();
  });

  it('saves config as new version successfully', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    mockAdmin.amasUpdateConfigWithNote.mockResolvedValue({ updated: true, versionHash: 'abc123', versionId: 2 });
    await renderPage();
    // 页头主操作按钮：保存为新版本
    await waitFor(() => expect(screen.getByRole('button', { name: '保存为新版本' })).toBeInTheDocument());
    // 初始无改动 → 按钮 disabled
    expect(screen.getByRole('button', { name: '保存为新版本' })).toBeDisabled();
    makeDirty();
    // dirty badge "有未保存修改"
    await waitFor(() => expect(screen.getAllByText('有未保存修改').length).toBeGreaterThan(0));
    const saveBtn = screen.getByRole('button', { name: '保存为新版本' });
    await waitFor(() => expect(saveBtn).not.toBeDisabled());
    fireEvent.click(saveBtn);
    // Confirm 对话框标题 "保存为新版本"
    await waitFor(() => expect(screen.getByRole('heading', { name: '保存为新版本' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(mockAdmin.amasUpdateConfigWithNote).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalledWith('配置已保存为新版本', expect.anything());
  });

  it('shows save failure toast', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    mockAdmin.amasUpdateConfigWithNote.mockRejectedValue(new Error('save fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '保存为新版本' })).toBeInTheDocument());
    makeDirty();
    const saveBtn = screen.getByRole('button', { name: '保存为新版本' });
    await waitFor(() => expect(saveBtn).not.toBeDisabled());
    fireEvent.click(saveBtn);
    await waitFor(() => expect(screen.getByRole('heading', { name: '保存为新版本' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('保存失败', 'save fail'));
  });

  it('rollback to baseline discards changes', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: /回滚到 baseline/ })).toBeInTheDocument());
    makeDirty();
    await waitFor(() => expect(screen.getAllByText('有未保存修改').length).toBeGreaterThan(0));
    const rollbackBtn = screen.getByRole('button', { name: /回滚到 baseline/ });
    await waitFor(() => expect(rollbackBtn).not.toBeDisabled());
    fireEvent.click(rollbackBtn);
    // resetDefaults 直接恢复 baseline → dirty badge 消失
    await waitFor(() => expect(screen.queryByText('有未保存修改')).not.toBeInTheDocument());
    expect(mockToast.info).toHaveBeenCalledWith('已回滚到 baseline', expect.anything());
  });

  it('reloads AMAS config successfully', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    mockAdmin.reloadAmas.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '热重载' })).toBeInTheDocument());
    makeDirty();
    const reloadBtn = screen.getByRole('button', { name: '热重载' });
    await waitFor(() => expect(reloadBtn).not.toBeDisabled());
    fireEvent.click(reloadBtn);
    // Confirm 标题 "热重载配置"，确认按钮文本 "热重载"
    await waitFor(() => expect(screen.getByRole('heading', { name: '热重载配置' })).toBeInTheDocument());
    // 弹窗内确认按钮（getAllByRole 后取最后一个，避免与页头按钮重名）
    const confirmBtns = screen.getAllByRole('button', { name: '热重载' });
    fireEvent.click(confirmBtns[confirmBtns.length - 1]);
    await waitFor(() => expect(mockAdmin.reloadAmas).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalledWith('已热重载', expect.anything());
  });

  it('shows reload failure toast', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    mockAdmin.reloadAmas.mockRejectedValue(new Error('reload fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '热重载' })).toBeInTheDocument());
    makeDirty();
    const reloadBtn = screen.getByRole('button', { name: '热重载' });
    await waitFor(() => expect(reloadBtn).not.toBeDisabled());
    fireEvent.click(reloadBtn);
    await waitFor(() => expect(screen.getByRole('heading', { name: '热重载配置' })).toBeInTheDocument());
    const confirmBtns = screen.getAllByRole('button', { name: '热重载' });
    fireEvent.click(confirmBtns[confirmBtns.length - 1]);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('热重载失败', 'reload fail'));
  });

  it('opens version drawer', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '版本历史' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '版本历史' }));
    // Drawer 标题 "版本历史"（heading）+ 空态文案
    await waitFor(() => expect(screen.getByRole('heading', { name: '版本历史' })).toBeInTheDocument());
    expect(screen.getByText('暂无版本')).toBeInTheDocument();
  });

  it('renders feature flag switches section', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    // 功能开关 section 标题 + 至少一个开关项标签
    await waitFor(() => expect(screen.getByText('功能开关')).toBeInTheDocument());
    expect(screen.getByText('策略集成')).toBeInTheDocument();
    expect(screen.getByText('MDM 记忆模型')).toBeInTheDocument();
  });

  it('renders ensemble algorithm mix section', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByText('算法配比')).toBeInTheDocument());
    expect(screen.getByText('启发式')).toBeInTheDocument();
    expect(screen.getByText('IGE')).toBeInTheDocument();
    expect(screen.getByText('SWD')).toBeInTheDocument();
  });

  it('shows change-preview empty state before edits', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    // 右侧「改动预览」Panel 初始空态
    await waitFor(() => expect(screen.getByText('改动预览')).toBeInTheDocument());
    expect(screen.getByText('尚无改动')).toBeInTheDocument();
  });
});

describe('AmasConfigPage — tabs / preset / drawer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockAdmin.amasSerializeToml.mockResolvedValue({ toml: '# stub\n' });
    mockAdmin.amasListVersions.mockResolvedValue([]);
    mockAdmin.amasGetCanary.mockResolvedValue({ canary: null });
  });

  it('switches to TOML editor tab and serializes config', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByText('可视化配置')).toBeInTheDocument());
    fireEvent.click(screen.getByText('高级 (TOML)'));
    // TomlEditor onMount 调用 amasSerializeToml + 渲染「配置源 (TOML)」
    await waitFor(() => expect(screen.getByText('配置源 (TOML)')).toBeInTheDocument());
    expect(mockAdmin.amasSerializeToml).toHaveBeenCalled();
  });

  it('switches to versions tab and shows empty state', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByText('可视化配置')).toBeInTheDocument());
    // tab 文本「版本历史」既出现在 tab 也出现在页头按钮，用 getAllByText 取 tab
    const versionTabs = screen.getAllByText('版本历史');
    fireEvent.click(versionTabs[0]);
    await waitFor(() => expect(screen.getByText('暂无版本')).toBeInTheDocument());
  });

  it('switches to canary tab and renders crowd-filter panel', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByText('可视化配置')).toBeInTheDocument());
    fireEvent.click(screen.getByText('灰度发布'));
    await waitFor(() => expect(screen.getByText('灰度配置')).toBeInTheDocument());
    expect(screen.getByText('受众估计')).toBeInTheDocument();
  });

  it('opens then closes version drawer via 关闭 button', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '版本历史' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '版本历史' }));
    await waitFor(() => expect(screen.getByText('暂无版本')).toBeInTheDocument());
    // Drawer 关闭按钮 aria-label="关闭"
    fireEvent.click(screen.getByRole('button', { name: '关闭' }));
    await waitFor(() => expect(screen.queryByText('暂无版本')).not.toBeInTheDocument());
  });

  it('applies a one-click preset and marks config dirty', async () => {
    mockAmas.getConfig.mockResolvedValue(freshConfig());
    await renderPage();
    // 一键预设区块 + 三个风格按钮
    await waitFor(() => expect(screen.getByText('一键预设')).toBeInTheDocument());
    expect(screen.getByText('稳妥')).toBeInTheDocument();
    expect(screen.getByText('激进')).toBeInTheDocument();
    // 点击「激进」预设 → applyPreset 写入 patch，config 偏离 baseline → dirty
    fireEvent.click(screen.getByText('激进'));
    await waitFor(() => expect(mockToast.info).toHaveBeenCalledWith(expect.stringContaining('激进'), expect.anything()));
    await waitFor(() => expect(screen.getAllByText('有未保存修改').length).toBeGreaterThan(0));
  });
});
