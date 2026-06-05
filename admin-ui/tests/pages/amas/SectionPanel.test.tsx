import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { SectionPanel } from '@/pages/amas/SectionPanel';

vi.mock('@/api/admin', () => ({
  adminApi: { amasExplainParam: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('SectionPanel(扁平 4 列 grid 重构)', () => {
  it('renders all section headings flat', () => {
    render(() => <SectionPanel config={{}} errors={[]} onChange={() => {}} />);
    // 全部 section 默认展开,不再是 collapsible
    expect(screen.getByText('记忆模型（FSRS-5）')).toBeInTheDocument();
    expect(screen.getByText('功能开关')).toBeInTheDocument();
    expect(screen.getByText('集成（Ensemble）')).toBeInTheDocument();
  });

  it('all sections expanded by default — every section content visible', () => {
    render(() => <SectionPanel config={{}} errors={[]} onChange={() => {}} />);
    // memoryModel 字段
    expect(screen.getAllByText(/目标长期留存率/).length).toBeGreaterThan(0);
    // featureFlags 字段(同时可见,无需展开)
    expect(screen.getByText('启用 3 路集成')).toBeInTheDocument();
  });

  it('shows error badge when section has errors', () => {
    render(() => (
      <SectionPanel
        config={{}}
        errors={[{ path: 'memoryModel.baseDesiredRetention', message: 'oops' }]}
        onChange={() => {}}
      />
    ));
    expect(screen.getByText('1 错')).toBeInTheDocument();
  });

  it('calls onChange when a param input changes', () => {
    const onChange = vi.fn();
    render(() => (
      <SectionPanel
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
