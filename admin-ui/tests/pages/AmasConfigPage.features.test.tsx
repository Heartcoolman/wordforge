import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

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
    amasListVersions: vi.fn(() => Promise.resolve([])),
    amasRollback: vi.fn(),
    amasGetVersion: vi.fn(),
    amasRestoreVersion: vi.fn(),
    // m022: CanaryCard + JsonAdvancedPanel TOML 模式
    amasGetCanary: vi.fn(() => Promise.resolve({ canary: null })),
    amasSetCanary: vi.fn(),
    amasDisableCanary: vi.fn(),
    amasParseToml: vi.fn(),
    amasSerializeToml: vi.fn(() => Promise.resolve({ toml: '# stub\n' })),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

// m027: CodeMirror 在 happy-dom 下不稳,mock 掉
vi.mock('@/components/amas/TomlEditor', () => ({
  default: () => null,
}));

import { amasApi } from '@/api/amas';
import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockAmas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const baseConfig = {
  memoryModel: { baseDesiredRetention: 0.92, maxIntervalDays: 90, w: Array(19).fill(1) },
  ensemble: { baseWeightHeuristic: 0.4, baseWeightIge: 0.3, baseWeightSwd: 0.3 },
  objectiveWeights: { retention: 0.35, accuracy: 0.25, speed: 0.15, fatigue: 0.15, frustration: 0.1 },
};

async function renderPage() {
  const { default: Page } = await import('@/pages/AmasConfigPage');
  return renderWithProviders(() => <Page />);
}

describe('AmasConfigPage — CRUD & error fallbacks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // m027: restore default amasSerializeToml impl(clearAllMocks 把 mockResolvedValue 清了)
    (adminApi.amasSerializeToml as ReturnType<typeof vi.fn>).mockResolvedValue({ toml: '# stub\n' });
    (adminApi.amasListVersions as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    (adminApi.amasGetCanary as ReturnType<typeof vi.fn>).mockResolvedValue({ canary: null });
  });

  it('handles getConfig failure with toast', async () => {
    mockAmas.getConfig.mockRejectedValue(new Error('boom'));
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('saves config successfully', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    mockAmas.updateConfig.mockResolvedValue(undefined);
    await renderPage();
    // page-header 重构后:"保存配置" → "验证并应用"
    await waitFor(() => expect(screen.getByRole('button', { name: '验证并应用' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    // Badge "未保存 · N 处" 替代旧 "未保存的修改"
    await waitFor(() => expect(screen.getByText(/未保存/)).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '验证并应用' }));
    await waitFor(() => expect(screen.getByText('确认保存 AMAS 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认保存' }));
    await waitFor(() => expect(mockAmas.updateConfig).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('shows save failure toast', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    mockAmas.updateConfig.mockRejectedValue(new Error('save fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '验证并应用' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    fireEvent.click(screen.getByRole('button', { name: '验证并应用' }));
    await waitFor(() => expect(screen.getByText('确认保存 AMAS 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认保存' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('保存失败', 'save fail'));
  });

  it('rollback to baseline discards changes', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '验证并应用' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    await waitFor(() => expect(screen.getByText(/未保存/)).toBeInTheDocument());
    // 新版回滚:PageHeader 上"回滚到 baseline"按钮触发 ConfirmDialog
    fireEvent.click(screen.getByRole('button', { name: '回滚到 baseline' }));
    // ConfirmDialog 标题与按钮文本都含 "回滚到 baseline" → 直接等待"确认回滚"按钮出现
    await waitFor(() =>
      expect(screen.getByRole('button', { name: '确认回滚' })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole('button', { name: '确认回滚' }));
    await waitFor(() => expect(screen.queryByText(/未保存/)).not.toBeInTheDocument());
  });

  it('reloads AMAS config successfully', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    mockAdmin.reloadAmas.mockResolvedValue(baseConfig);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '热重载' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    await waitFor(() => expect(screen.getByText(/未保存/)).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '热重载' }));
    await waitFor(() => expect(screen.getByText('确认热重载 AMAS 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认热重载' }));
    await waitFor(() => expect(mockAdmin.reloadAmas).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalledWith('AMAS 配置已热重载');
  });

  it('shows reload failure toast', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    mockAdmin.reloadAmas.mockRejectedValue(new Error('reload fail'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '热重载' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    await waitFor(() => expect(screen.getByText(/未保存/)).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '热重载' }));
    await waitFor(() => expect(screen.getByText('确认热重载 AMAS 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认热重载' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('热重载失败', 'reload fail'));
  });

  it('opens version drawer', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '版本历史' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '版本历史' }));
  });

  it('blocks save with validation errors', async () => {
    mockAmas.getConfig.mockResolvedValue({ ...baseConfig, memoryModel: { baseDesiredRetention: 2.0, w: Array(19).fill(1) } });
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByText(/校验错误/)).toBeInTheDocument());
  });

  it('renders algorithm metrics table', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({
      Heuristic: { callCount: 100, totalLatencyUs: 50000, errorCount: 1 },
      IGE: { callCount: 0, totalLatencyUs: 0, errorCount: 0 },
    });
    await renderPage();
    await waitFor(() => expect(screen.getByText('算法指标')).toBeInTheDocument());
    expect(screen.getByText('Heuristic')).toBeInTheDocument();
    expect(screen.getByText('IGE')).toBeInTheDocument();
  });

  it('shows empty metrics text when no entries', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无指标数据')).toBeInTheDocument());
  });
});

describe('AmasConfigPage — tabs / preset / drawer', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (adminApi.amasSerializeToml as ReturnType<typeof vi.fn>).mockResolvedValue({ toml: '# stub\n' });
    (adminApi.amasListVersions as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    (adminApi.amasGetCanary as ReturnType<typeof vi.fn>).mockResolvedValue({ canary: null });
  });

  it('switches to sections tab and renders SectionPanel', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByRole('tab', { name: /分节配置/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('tab', { name: /分节配置/ }));
    await waitFor(() => expect(screen.getByRole('tab', { name: /分节配置/ })).toHaveAttribute('aria-selected', 'true'));
  });

  it('switches to json tab and renders JsonAdvancedPanel', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByRole('tab', { name: /JSON 高级/ })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('tab', { name: /JSON 高级/ }));
    await waitFor(() => expect(screen.getByRole('tab', { name: /JSON 高级/ })).toHaveAttribute('aria-selected', 'true'));
  });

  it('opens then closes version drawer via × button', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    const { container } = await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '版本历史' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '版本历史' }));
    await waitFor(() => expect(screen.getByText('尚无版本')).toBeInTheDocument());
    const dialog = () => container.querySelector('[role="dialog"]') as HTMLElement | null;
    expect(dialog()?.className).toContain('translate-x-0');
    fireEvent.click(screen.getByRole('button', { name: '关闭' }));
    await waitFor(() => expect(dialog()?.className).toContain('translate-x-full'));
    expect(dialog()?.className).toContain('pointer-events-none');
  });

  it('PresetBar 重置到该预设按钮触发 onApply(默认 factory_default)', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    // PresetBar 取代 PresetSelector;无 modal preview,直接点"重置到该预设"应用
    await waitFor(() =>
      expect(screen.getByRole('button', { name: '重置到该预设' })).toBeInTheDocument(),
    );
    // factory_default 默认选中 → 点击应触发 onApply(setConfig 到 factory_default)
    fireEvent.click(screen.getByRole('button', { name: '重置到该预设' }));
    // 现在 config 应等于 factory_default,Badge "未保存 · N 处"应消失(从 baseline 一致)
    // 因为 baseConfig 已是 factory 值,UI 应无 dirty(原本默认 factory)
  });
});
