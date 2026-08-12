import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent, render } from '@solidjs/testing-library';
import type { ProbeRow } from '@/types/probeTelemetry';

const updateSamplingRule = vi.fn();

vi.mock('@/api/probeTelemetry', () => ({
  probeTelemetryApi: {
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

function mkProbe(over: Partial<ProbeRow> = {}): ProbeRow {
  return {
    key: 'click',
    label: '点击行为',
    desc: 'telemetry_summaries.click_count',
    count24h: 5200,
    eps: 0.06,
    lastTs: '2026-05-29T11:59:50Z',
    sampleRate: 0.35,
    locked: false,
    enabled: true,
    samplingEventType: 'periodic',
    ...over,
  };
}

async function renderSlider(probe: ProbeRow, onSaved?: () => void) {
  const { SamplingSlider } = await import('@/pages/probe/SamplingSlider');
  return render(() => <SamplingSlider probe={probe} onSaved={onSaved} />);
}

describe('SamplingSlider', () => {
  beforeEach(() => vi.clearAllMocks());

  it('可调探针：初始百分比来自 sampleRate，滑杆可用', async () => {
    const { container } = await renderSlider(mkProbe());
    const input = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    expect(input.disabled).toBe(false);
    expect(container.querySelector('.pb-sample')).not.toHaveClass('is-locked');
    expect(container.querySelector('.pct')).toHaveTextContent('35%');
  });

  it('locked=true → 滑杆 disabled 且容器加 is-locked', async () => {
    const { container } = await renderSlider(mkProbe({ locked: true, sampleRate: 1 }));
    expect((screen.getByLabelText('点击行为 采样率') as HTMLInputElement).disabled).toBe(true);
    expect(container.querySelector('.pb-sample')).toHaveClass('is-locked');
    expect(container.querySelector('.pct')).toHaveTextContent('100%');
  });

  it('无 samplingEventType（核心数据派生探针）同样锁定', async () => {
    const probe = mkProbe({ locked: false });
    delete probe.samplingEventType;
    const { container } = await renderSlider(probe);
    expect((screen.getByLabelText('点击行为 采样率') as HTMLInputElement).disabled).toBe(true);
    expect(container.querySelector('.pb-sample')).toHaveClass('is-locked');
  });

  it('拖动后 debounce 提交：PATCH 对应 event_type + 成功 toast + onSaved', async () => {
    updateSamplingRule.mockResolvedValue({});
    const onSaved = vi.fn();
    const { container } = await renderSlider(mkProbe(), onSaved);
    const input = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '55' } });
    // 百分比立即回显，请求要等 debounce
    expect(container.querySelector('.pct')).toHaveTextContent('55%');
    expect(updateSamplingRule).not.toHaveBeenCalled();
    await waitFor(
      () => expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { sampleRate: 0.55 }),
      { timeout: 2000 },
    );
    await waitFor(() =>
      expect(toastSuccess).toHaveBeenCalledWith('采样率已更新', '点击行为 → 55%'),
    );
    expect(onSaved).toHaveBeenCalledTimes(1);
  });

  it('连续拖动只提交最后一次（debounce 合并）', async () => {
    updateSamplingRule.mockResolvedValue({});
    await renderSlider(mkProbe());
    const input = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '10' } });
    fireEvent.input(input, { target: { value: '20' } });
    fireEvent.input(input, { target: { value: '30' } });
    await waitFor(() => expect(updateSamplingRule).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(updateSamplingRule).toHaveBeenCalledWith('periodic', { sampleRate: 0.3 });
  });

  it('提交失败 → 百分比回滚到原值 + 错误 toast 带原因', async () => {
    updateSamplingRule.mockRejectedValue(new Error('locked by server'));
    const onSaved = vi.fn();
    const { container } = await renderSlider(mkProbe(), onSaved);
    const input = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '80' } });
    expect(container.querySelector('.pct')).toHaveTextContent('80%');
    await waitFor(
      () => expect(toastError).toHaveBeenCalledWith('采样率更新失败', 'locked by server'),
      { timeout: 2000 },
    );
    expect(container.querySelector('.pct')).toHaveTextContent('35%');
    expect(onSaved).not.toHaveBeenCalled();
  });

  it('非 Error 拒绝 → 错误 toast 回落"请求失败"', async () => {
    updateSamplingRule.mockRejectedValue('boom');
    await renderSlider(mkProbe());
    fireEvent.input(screen.getByLabelText('点击行为 采样率'), { target: { value: '5' } });
    await waitFor(() => expect(toastError).toHaveBeenCalledWith('采样率更新失败', '请求失败'), {
      timeout: 2000,
    });
  });

  it('锁定探针：disabled 拦住用户输入，事件也到不了 handler', async () => {
    const { container } = await renderSlider(mkProbe({ locked: true, sampleRate: 1 }));
    const input = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '10' } });
    // disabled 元素不派发事件，回显与请求都不该动
    expect(container.querySelector('.pct')).toHaveTextContent('100%');
    await new Promise((r) => setTimeout(r, 400));
    expect(updateSamplingRule).not.toHaveBeenCalled();
  });

  it('locked 探针即便 disabled 被绕过，handler 内的守卫仍拒绝提交', async () => {
    const { container } = await renderSlider(mkProbe({ locked: true, sampleRate: 1 }));
    const input = screen.getByLabelText('点击行为 采样率') as HTMLInputElement;
    // 手动摘掉 disabled 模拟"绕过 UI 禁用"，专测 onInput 里的 locked() 守卫
    input.removeAttribute('disabled');
    fireEvent.input(input, { target: { value: '10' } });
    expect(container.querySelector('.pct')).toHaveTextContent('10%');
    await new Promise((r) => setTimeout(r, 400));
    expect(updateSamplingRule).not.toHaveBeenCalled();
  });

  it('卸载时清掉未触发的 debounce 定时器，不再发请求', async () => {
    updateSamplingRule.mockResolvedValue({});
    const { unmount } = await renderSlider(mkProbe());
    fireEvent.input(screen.getByLabelText('点击行为 采样率'), { target: { value: '60' } });
    unmount();
    await new Promise((r) => setTimeout(r, 400));
    expect(updateSamplingRule).not.toHaveBeenCalled();
  });
});
