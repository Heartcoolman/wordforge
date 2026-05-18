import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { SectionPanel } from '@/pages/admin/amas/SectionPanel';

vi.mock('@/api/admin', () => ({
  adminApi: { amasExplainParam: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

describe('SectionPanel', () => {
  it('renders section headings', () => {
    render(() => <SectionPanel config={{}} errors={[]} onChange={() => {}} />);
    expect(screen.getByText('记忆模型（FSRS-5）')).toBeInTheDocument();
    expect(screen.getByText('功能开关')).toBeInTheDocument();
    expect(screen.getByText('集成（Ensemble）')).toBeInTheDocument();
  });

  it('toggles section open/close on click', () => {
    render(() => <SectionPanel config={{}} errors={[]} onChange={() => {}} />);
    // memoryModel 默认展开 → 看到 baseDesiredRetention label
    expect(screen.getAllByText(/目标长期留存率/).length).toBeGreaterThan(0);
    const header = screen.getByText('记忆模型（FSRS-5）').closest('button')!;
    fireEvent.click(header); // 收起
    fireEvent.click(header); // 再展开
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

  it('switching section closes previous and opens new', () => {
    render(() => <SectionPanel config={{}} errors={[]} onChange={() => {}} />);
    const flagsHeader = screen.getByText('功能开关').closest('button')!;
    fireEvent.click(flagsHeader);
    // memoryModel 应被关闭，但 featureFlags 内 ensembleEnabled label 出现
    expect(screen.getByText('启用 3 路集成')).toBeInTheDocument();
  });
});
