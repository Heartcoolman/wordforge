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
    amasListVersions: vi.fn(() => Promise.resolve([])),
    amasRollback: vi.fn(),
    amasGetVersion: vi.fn(),
    amasRestoreVersion: vi.fn(),
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

const baseConfig = {
  memoryModel: { baseDesiredRetention: 0.92, maxIntervalDays: 90, w: Array(19).fill(1) },
  ensemble: { baseWeightHeuristic: 0.4, baseWeightIge: 0.3, baseWeightSwd: 0.3 },
  objectiveWeights: { retention: 0.35, accuracy: 0.25, speed: 0.15, fatigue: 0.15, frustration: 0.1 },
};

async function renderPage() {
  const { default: Page } = await import('@/pages/admin/AmasConfigPage');
  return renderWithProviders(() => <Page />);
}

describe('AmasConfigPage — CRUD & error fallbacks', () => {
  beforeEach(() => vi.clearAllMocks());

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
    await waitFor(() => expect(screen.getByRole('button', { name: '保存配置' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    await waitFor(() => expect(screen.getByText('未保存的修改')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    // 弹出 ConfirmDialog 二次确认
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
    await waitFor(() => expect(screen.getByRole('button', { name: '保存配置' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }));
    await waitFor(() => expect(screen.getByText('确认保存 AMAS 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认保存' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('保存失败', 'save fail'));
  });

  it('discards changes via 放弃修改', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '保存配置' })).toBeInTheDocument());
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    await waitFor(() => expect(screen.getByText('放弃修改')).toBeInTheDocument());
    fireEvent.click(screen.getByText('放弃修改'));
    await waitFor(() => expect(screen.queryByText('未保存的修改')).not.toBeInTheDocument());
  });

  it('reloads AMAS config successfully', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    mockAdmin.reloadAmas.mockResolvedValue(baseConfig);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '热重载' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '热重载' }));
    // 弹出 ConfirmDialog
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
    fireEvent.click(screen.getByRole('button', { name: '热重载' }));
    await waitFor(() => expect(screen.getByText('确认热重载 AMAS 配置')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认热重载' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalledWith('热重载失败', 'reload fail'));
  });

  it('opens version drawer', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByText('版本历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('版本历史'));
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
  beforeEach(() => vi.clearAllMocks());

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
    await renderPage();
    await waitFor(() => expect(screen.getByText('版本历史')).toBeInTheDocument());
    fireEvent.click(screen.getByText('版本历史'));
    await waitFor(() => expect(screen.getByText('尚无版本')).toBeInTheDocument());
    const closeBtns = screen.getAllByText('×');
    fireEvent.click(closeBtns[0]);
    await waitFor(() => expect(screen.queryByText('尚无版本')).not.toBeInTheDocument());
  });

  it('applies a preset via PresetSelector confirm button', async () => {
    mockAmas.getConfig.mockResolvedValue(baseConfig);
    mockAmas.getMetrics.mockResolvedValue({});
    await renderPage();
    await waitFor(() => expect(screen.getByText('出厂默认')).toBeInTheDocument());
    fireEvent.click(screen.getByText('出厂默认'));
    await waitFor(() => expect(screen.getByText('应用到表单')).toBeInTheDocument());
    fireEvent.click(screen.getByText('应用到表单'));
    await waitFor(() => expect(screen.queryByText('应用到表单')).not.toBeInTheDocument());
  });
});
