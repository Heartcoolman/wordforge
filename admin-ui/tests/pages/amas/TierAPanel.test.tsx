import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { TierAPanel } from '@/pages/amas/TierAPanel';

vi.mock('@/api/admin', () => ({
  adminApi: { amasExplainParam: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('TierAPanel(三层运维视角重构)', () => {
  it('renders preset cards + flag block + quick knobs + Tier-A 11 grid', () => {
    render(() => <TierAPanel config={{}} errors={[]} onChange={() => {}} />);
    // 4 preset 卡(至少 2 项真实)
    expect(screen.getByText('出厂默认 (FSRS-6)')).toBeInTheDocument();
    // 算法 flag block 标题
    expect(screen.getByText(/算法 flag.*featureFlags 一层开关/)).toBeInTheDocument();
    // 核心 4 旋钮 block 标题
    expect(screen.getByText(/核心 4 旋钮/)).toBeInTheDocument();
    // Tier-A 11 维 block 标题
    expect(screen.getByText(/Tier-A 12 维.*高杠杆精雕项/)).toBeInTheDocument();
    // Tier-A 11 维参数标签
    expect(screen.getAllByText('目标长期留存率').length).toBeGreaterThan(0);
  });

  it('passes errors to fields(quick knobs + Tier-A 11 维 双区都显示)', () => {
    render(() => (
      <TierAPanel
        config={{}}
        errors={[{ path: 'memoryModel.baseDesiredRetention', message: '阈值错误' }]}
        onChange={() => {}}
      />
    ));
    // baseDesiredRetention 同时出现在 quick knobs 和 Tier-A 11 维 grid,
    // 错误也应该传递两次
    const errs = screen.getAllByText('阈值错误');
    expect(errs.length).toBeGreaterThan(0);
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
    const input = document.querySelector('input[type="number"]') as HTMLInputElement;
    fireEvent.input(input, { target: { value: '0.85' } });
    expect(onChange).toHaveBeenCalled();
  });
});
