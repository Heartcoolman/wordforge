import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { TierAPanel } from '@/pages/admin/amas/TierAPanel';

vi.mock('@/api/admin', () => ({
  adminApi: { amasExplainParam: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('TierAPanel', () => {
  it('renders Tier-A heading and all 11 params', () => {
    render(() => <TierAPanel config={{}} errors={[]} onChange={() => {}} />);
    expect(screen.getByText(/重点参数（Tier-A 11 维）/)).toBeInTheDocument();
    expect(screen.getByText('目标长期留存率')).toBeInTheDocument();
    expect(screen.getByText('最大调度间隔（天）')).toBeInTheDocument();
  });

  it('passes errors to fields', () => {
    render(() => (
      <TierAPanel
        config={{}}
        errors={[{ path: 'memoryModel.baseDesiredRetention', message: '阈值错误' }]}
        onChange={() => {}}
      />
    ));
    expect(screen.getByText('阈值错误')).toBeInTheDocument();
  });

  it('calls onChange on input', () => {
    const onChange = vi.fn();
    render(() => (
      <TierAPanel
        config={{ memoryModel: { baseDesiredRetention: 0.92 } }}
        errors={[]}
        onChange={onChange}
      />
    ));
    const input = screen.getByDisplayValue('0.92') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    expect(onChange).toHaveBeenCalled();
  });
});
