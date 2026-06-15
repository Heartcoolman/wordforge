import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { ParamField } from '@/pages/amas/ParamField';
import type { ParamMeta } from '@/pages/amas/schema';

vi.mock('@/api/admin', () => ({
  adminApi: { amasExplainParam: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const numMeta: ParamMeta = {
  path: 'memoryModel.alpha',
  label_zh: 'α',
  type: 'number',
  min: 0, max: 1, step: 0.01,
  default: 0.5,
  description_zh: '学习率',
  affects: ['retention', 'fatigue'],
  sensitivity: 'ultra',
};
const intMeta: ParamMeta = {
  path: 'ensemble.warmupSamples',
  label_zh: '热身样本',
  type: 'integer',
  min: 1, max: 100,
  default: 20,
  description_zh: '样本数',
  sensitivity: 'high',
};
const boolMeta: ParamMeta = {
  path: 'featureFlags.enabled',
  label_zh: '启用',
  type: 'bool',
  default: false,
  description_zh: '开关',
};

describe('ParamField', () => {
  beforeEach(() => vi.clearAllMocks());

  it('renders number field with label, description and default chip', () => {
    render(() => <ParamField meta={numMeta} value={0.5} onChange={() => {}} />);
    expect(screen.getByText('α')).toBeInTheDocument();
    expect(screen.getByText('学习率')).toBeInTheDocument();
    expect(screen.getByText(/默认/)).toBeInTheDocument();
    expect(screen.getByText(/影响/)).toBeInTheDocument();
  });

  it('compact mode hides description', () => {
    render(() => <ParamField meta={numMeta} value={0.5} compact onChange={() => {}} />);
    expect(screen.queryByText('学习率')).not.toBeInTheDocument();
  });

  it('renders sensitivity badge for ultra', () => {
    render(() => <ParamField meta={numMeta} value={0.5} onChange={() => {}} />);
    expect(screen.getByText('高敏')).toBeInTheDocument();
  });

  it('renders sensitivity badge for high', () => {
    render(() => <ParamField meta={intMeta} value={20} onChange={() => {}} />);
    expect(screen.getByText('中敏')).toBeInTheDocument();
  });

  it('handles number input change', () => {
    const onChange = vi.fn();
    render(() => <ParamField meta={numMeta} value={0.5} onChange={onChange} />);
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.75' } });
    expect(onChange).toHaveBeenCalledWith(0.75);
  });

  it('handles integer input correctly', () => {
    const onChange = vi.fn();
    render(() => <ParamField meta={intMeta} value={20} onChange={onChange} />);
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '50' } });
    expect(onChange).toHaveBeenCalledWith(50);
  });

  it('ignores empty input', () => {
    const onChange = vi.fn();
    render(() => <ParamField meta={numMeta} value={0.5} onChange={onChange} />);
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('ignores non-finite input', () => {
    const onChange = vi.fn();
    render(() => <ParamField meta={numMeta} value={0.5} onChange={onChange} />);
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: 'abc' } });
    expect(onChange).not.toHaveBeenCalled();
  });

  it('renders bool field as Switch and toggles', () => {
    const onChange = vi.fn();
    render(() => <ParamField meta={boolMeta} value={false} onChange={onChange} />);
    expect(screen.getByText('已停用')).toBeInTheDocument();
    const sw = document.querySelector('button[role="switch"]') as HTMLButtonElement;
    fireEvent.click(sw);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it('renders enabled switch label when value=true', () => {
    render(() => <ParamField meta={boolMeta} value={true} onChange={() => {}} />);
    expect(screen.getByText('已启用')).toBeInTheDocument();
  });

  it('clicking 默认 resets to default', () => {
    const onChange = vi.fn();
    render(() => <ParamField meta={numMeta} value={0.9} onChange={onChange} />);
    fireEvent.click(screen.getByTitle('恢复为出厂默认'));
    expect(onChange).toHaveBeenCalledWith(0.5);
  });

  it('shows changed dot when value differs from default', () => {
    render(() => <ParamField meta={numMeta} value={0.9} onChange={() => {}} />);
    const dot = document.querySelector('span[title="已修改"]');
    expect(dot).toHaveClass('bg-accent');
  });

  it('shows error message when prop.error', () => {
    render(() => <ParamField meta={numMeta} value={0.5} error="超出范围" onChange={() => {}} />);
    expect(screen.getByText('超出范围')).toBeInTheDocument();
  });

  it('asks AI and renders explanation', async () => {
    mockApi.amasExplainParam.mockResolvedValue({ explanation: '这是 α 学习率参数' });
    render(() => <ParamField meta={numMeta} value={0.5} onChange={() => {}} />);
    fireEvent.click(screen.getByText('问 AI'));
    await waitFor(() => expect(screen.getByText(/这是 α 学习率参数/)).toBeInTheDocument());
  });

  it('handles AI error', async () => {
    mockApi.amasExplainParam.mockRejectedValue(new Error('boom'));
    // 注意：path 不同避免 explainCache 缓存命中
    const meta = { ...numMeta, path: 'memoryModel.fail' };
    render(() => <ParamField meta={meta} value={0.5} onChange={() => {}} />);
    fireEvent.click(screen.getByText('问 AI'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('uses cached AI explanation on second call', async () => {
    mockApi.amasExplainParam.mockResolvedValue({ explanation: 'cached val' });
    const meta = { ...numMeta, path: 'memoryModel.cached' };
    const { unmount } = render(() => <ParamField meta={meta} value={0.5} onChange={() => {}} />);
    fireEvent.click(screen.getByText('问 AI'));
    await waitFor(() => expect(mockApi.amasExplainParam).toHaveBeenCalledTimes(1));
    unmount();

    render(() => <ParamField meta={meta} value={0.5} onChange={() => {}} />);
    fireEvent.click(screen.getByText('问 AI'));
    // 仍只一次（命中缓存）
    expect(mockApi.amasExplainParam).toHaveBeenCalledTimes(1);
  });
});
