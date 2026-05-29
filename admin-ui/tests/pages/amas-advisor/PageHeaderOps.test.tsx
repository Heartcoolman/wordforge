import { describe, it, expect, vi } from 'vitest';
import { screen, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { PageHeaderOps } from '@/pages/amas-advisor/PageHeaderOps';

describe('PageHeaderOps', () => {
  it('渲染三个操作并回调', () => {
    const onToggle = vi.fn();
    const onRun = vi.fn();
    const onApproveAll = vi.fn();
    renderWithProviders(() => (
      <PageHeaderOps
        advisorEnabled={true}
        running={false}
        pendingCount={2}
        onToggleAutoScan={onToggle}
        onRunNow={onRun}
        onApproveAll={onApproveAll}
      />
    ));
    fireEvent.click(screen.getByRole('switch', { name: /自动巡查/ }));
    expect(onToggle).toHaveBeenCalledWith(false);
    fireEvent.click(screen.getByRole('button', { name: /立即触发巡查/ }));
    expect(onRun).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: /接受全部待审/ }));
    expect(onApproveAll).toHaveBeenCalled();
  });

  it('pendingCount 为 0 时禁用"接受全部待审"', () => {
    renderWithProviders(() => (
      <PageHeaderOps advisorEnabled={false} running={false} pendingCount={0}
        onToggleAutoScan={() => {}} onRunNow={() => {}} onApproveAll={() => {}} />
    ));
    expect(screen.getByRole('button', { name: /接受全部待审/ })).toBeDisabled();
  });
});
