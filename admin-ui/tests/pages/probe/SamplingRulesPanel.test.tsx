import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@solidjs/testing-library';
import type { SamplingResponse } from '@/types/probeTelemetry';

const sampling = vi.fn();
const updateSamplingRule = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
    sampling: (...a: unknown[]) => sampling(...a),
    updateSamplingRule: (...a: unknown[]) => updateSamplingRule(...a),
  },
}));

const toastSuccess = vi.fn();
const toastError = vi.fn();
vi.mock('@/stores/ui', () => ({
  uiStore: {
    toast: {
      success: (...a: unknown[]) => toastSuccess(...a),
      error: (...a: unknown[]) => toastError(...a),
      warning: vi.fn(),
      info: vi.fn(),
    },
  },
}));

// on_demand 锁定；periodic 可调启用；session_start 可调已暂停；* 可调但采样率 0
const RULES: SamplingResponse = {
  globalDefault: 0.2,
  rules: [
    { eventType: 'on_demand', sampleRate: 1, enabled: true, locked: true, priority: 10 },
    { eventType: 'periodic', sampleRate: 0.35, enabled: true, locked: false, priority: 100, target: 'click' },
    { eventType: 'session_start', sampleRate: 0.5, enabled: false, locked: false, priority: 200 },
    { eventType: '*', sampleRate: 0, enabled: true, locked: false, priority: 1000 },
  ],
};

async function renderPanel() {
  const { SamplingRulesPanel } = await import('@/pages/probe/SamplingRulesPanel');
  return render(() => <SamplingRulesPanel />);
}

/** 行索引与 RULES 顺序一致：0 on_demand / 1 periodic / 2 session_start / 3 * */
const editButtons = () => screen.getAllByRole('button', { name: '编辑' });
const switches = () => screen.getAllByRole('switch');

describe('SamplingRulesPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sampling.mockResolvedValue(RULES);
    updateSamplingRule.mockResolvedValue({
      eventType: 'periodic', sampleRate: 0.5, enabled: true, locked: false, updatedAt: '2026-05-29T12:00:00Z',
    });
  });

  it('加载中：Spinner + 头部"加载中"占位', async () => {
    sampling.mockReturnValue(new Promise<never>(() => {}));
    const { container } = await renderPanel();
    expect(screen.getByRole('status')).toBeInTheDocument();
    // Spinner 的 sr-only 文案同为"加载中"，这里只认头部 meta 位
    expect(container.querySelector('.pb-card-header .meta')?.textContent).toBe('加载中');
  });

  it('规则为空渲染空态', async () => {
    sampling.mockResolvedValue({ globalDefault: 0.2, rules: [] });
    await renderPanel();
    await waitFor(() => expect(screen.getByText('暂无采样规则')).toBeInTheDocument());
    expect(screen.getByText('probe_sampling_config 表为空')).toBeInTheDocument();
  });

  it('头部展示全局默认采样率，新增规则按钮永久禁用', async () => {
    await renderPanel();
    await waitFor(() =>
      expect(screen.getByText('从上到下命中即停 · 未命中走全局默认 20%')).toBeInTheDocument(),
    );
    const addBtn = screen.getByRole('button', { name: '+ 新增规则' }) as HTMLButtonElement;
    expect(addBtn.disabled).toBe(true);
    expect(addBtn).toHaveAttribute('title', '规则集随数据库迁移预置，当前仅支持编辑/暂停已有规则');
  });

  it('逐行渲染中文类型、绑定探针、锁定/暂停标注与采样率', async () => {
    const { container } = await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    expect(screen.getByText('按需上报')).toBeInTheDocument();
    expect(screen.getByText('会话开始')).toBeInTheDocument();
    // 未登记类型回退原值
    expect(screen.getByText('*')).toBeInTheDocument();
    // target / locked / enabled 组合出的说明行
    expect(screen.getByText('绑定探针 click')).toBeInTheDocument();
    expect(screen.getByText('— · 核心数据强制')).toBeInTheDocument();
    expect(screen.getByText('— · 已暂停')).toBeInTheDocument();
    // 采样率百分比
    expect(screen.getByText('35%')).toBeInTheDocument();
    expect(screen.getByText('100%')).toBeInTheDocument();
    // off 行 = 已暂停(session_start) + 采样率 0(*)
    expect(container.querySelectorAll('.pb-rule.is-off').length).toBe(2);
  });

  it('locked 行禁止编辑与暂停：显示锁标、Switch disabled', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('按需上报')).toBeInTheDocument());
    // 4 条规则里只有 1 条 locked → 3 个编辑按钮 + 1 个锁标
    expect(editButtons().length).toBe(3);
    expect(document.querySelectorAll('.lock-badge').length).toBe(1);
    const [lockedSwitch, periodicSwitch] = switches();
    expect((lockedSwitch as HTMLButtonElement).disabled).toBe(true);
    expect(periodicSwitch).toHaveAttribute('aria-checked', 'true');
    fireEvent.click(lockedSwitch);
    expect(updateSamplingRule).not.toHaveBeenCalled();
  });

  it('进入编辑态：输入框带当前采样率，越界值被钳到 0~100', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(editButtons()[0]); // periodic

    const input = (await screen.findByLabelText('periodic 采样率')) as HTMLInputElement;
    expect(input.value).toBe('35');
    fireEvent.input(input, { target: { value: '200' } });
    await waitFor(() => expect(input.value).toBe('100'));
    // 非数字：number 输入框会被浏览器清空 → Number('') || 0 → 0
    fireEvent.input(input, { target: { value: 'abc' } });
    await waitFor(() => expect(input.value).toBe('0'));
    fireEvent.input(input, { target: { value: '80' } });
    await waitFor(() => expect(input.value).toBe('80'));
    fireEvent.input(input, { target: { value: '-5' } });
    await waitFor(() => expect(input.value).toBe('0'));
    // 编辑态下该行不再显示只读百分比标签
    expect(screen.queryByText('35%')).toBeNull();
  });

  it('保存成功：PATCH 新采样率 + toast + 退出编辑 + 重新拉取', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(editButtons()[0]);
    const input = (await screen.findByLabelText('periodic 采样率')) as HTMLInputElement;
    fireEvent.input(input, { target: { value: '50' } });

    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { sampleRate: 0.5 }));
    await waitFor(() => expect(toastSuccess).toHaveBeenCalledWith('采样率已更新', 'periodic → 50%'));
    await waitFor(() => expect(screen.queryByLabelText('periodic 采样率')).toBeNull());
    await waitFor(() => expect(sampling).toHaveBeenCalledTimes(2));
  });

  it('保存失败：错误 toast，保持编辑态且不刷新数据', async () => {
    updateSamplingRule.mockRejectedValue(new Error('rate rejected'));
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(editButtons()[0]);
    const input = (await screen.findByLabelText('periodic 采样率')) as HTMLInputElement;
    fireEvent.input(input, { target: { value: '80' } });

    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(toastError).toHaveBeenCalledWith('更新失败', 'rate rejected'));
    expect(screen.getByLabelText('periodic 采样率')).toBeInTheDocument();
    expect(sampling).toHaveBeenCalledTimes(1);
  });

  it('保存失败且非 Error 抛出：回退"请求失败"文案', async () => {
    updateSamplingRule.mockRejectedValue('plain string');
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(editButtons()[0]);
    const input = (await screen.findByLabelText('periodic 采样率')) as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(toastError).toHaveBeenCalledWith('更新失败', '请求失败'));
  });

  it('采样率未变更直接保存：不发请求，直接退出编辑', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(editButtons()[0]);
    await screen.findByLabelText('periodic 采样率');
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => expect(screen.queryByLabelText('periodic 采样率')).toBeNull());
    expect(updateSamplingRule).not.toHaveBeenCalled();
    expect(toastSuccess).not.toHaveBeenCalled();
  });

  it('取消编辑：不发请求且恢复只读百分比', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(editButtons()[0]);
    const input = (await screen.findByLabelText('periodic 采样率')) as HTMLInputElement;
    fireEvent.input(input, { target: { value: '90' } });
    fireEvent.click(screen.getByRole('button', { name: '取消' }));
    await waitFor(() => expect(screen.getByText('35%')).toBeInTheDocument());
    expect(updateSamplingRule).not.toHaveBeenCalled();
  });

  it('保存进行中：保存/取消按钮禁用', async () => {
    updateSamplingRule.mockReturnValue(new Promise<never>(() => {}));
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(editButtons()[0]);
    const input = (await screen.findByLabelText('periodic 采样率')) as HTMLInputElement;
    fireEvent.input(input, { target: { value: '60' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() =>
      expect((screen.getByRole('button', { name: '保存' }) as HTMLButtonElement).disabled).toBe(true),
    );
    expect((screen.getByRole('button', { name: '取消' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('暂停可调规则：PATCH enabled=false + toast + 重新拉取', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(switches()[1]); // periodic，当前启用
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { enabled: false }));
    await waitFor(() => expect(toastSuccess).toHaveBeenCalledWith('规则已暂停', 'periodic'));
    await waitFor(() => expect(sampling).toHaveBeenCalledTimes(2));
  });

  it('恢复已暂停规则：PATCH enabled=true + 启用 toast', async () => {
    await renderPanel();
    await waitFor(() => expect(screen.getByText('会话开始')).toBeInTheDocument());
    fireEvent.click(switches()[2]); // session_start，当前已暂停
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledWith('session_start', { enabled: true }));
    await waitFor(() => expect(toastSuccess).toHaveBeenCalledWith('规则已启用', 'session_start'));
  });

  it('暂停失败：错误 toast 且不刷新数据', async () => {
    updateSamplingRule.mockRejectedValue(new Error('locked by backend'));
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(switches()[1]);
    await waitFor(() => expect(toastError).toHaveBeenCalledWith('操作失败', 'locked by backend'));
    expect(sampling).toHaveBeenCalledTimes(1);
  });

  it('暂停失败且非 Error 抛出：回退"请求失败"文案', async () => {
    updateSamplingRule.mockRejectedValue({ code: 409 });
    await renderPanel();
    await waitFor(() => expect(screen.getByText('周期上报')).toBeInTheDocument());
    fireEvent.click(switches()[1]);
    await waitFor(() => expect(toastError).toHaveBeenCalledWith('操作失败', '请求失败'));
  });
});
