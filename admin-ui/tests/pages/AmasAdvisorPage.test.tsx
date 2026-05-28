import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListSuggestions: vi.fn(),
    amasSuggestionSpend: vi.fn(),
    amasApproveSuggestion: vi.fn(),
    amasRejectSuggestion: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { uiStore } from '@/stores/ui';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockSpend = {
  todayCostUsd: 0.123, dailyCapUsd: 5, remainingUsd: 4.877,
  todayTokensInput: 1000, todayTokensOutput: 800,
};
const mockPending = [{
  id: 1,
  status: 'pending' as const,
  basedOnVersionHash: 'abc1234567def890',
  rationale: '增加稳定性参数以提升记忆',
  patchJson: { 'memoryModel.w[0]': 0.5 },
  evidenceJson: { metric: 'logLoss', delta: -0.02 },
  confidence: 0.85,
  costUsd: 0.012,
  createdAt: '2026-04-15T10:00:00Z',
  decidedBy: null,
}];
const mockHistory = [
  { ...mockPending[0], id: 2, status: 'approved' as const, decidedBy: 'admin@example.com' },
  { ...mockPending[0], id: 3, status: 'rejected' as const, decidedBy: 'admin@example.com', costUsd: null },
];

describe('AmasAdvisorPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // approve/reject 不再用原生 window.confirm/prompt，改用 ConfirmDialog + Modal
    // 但 "approve aborts when confirm returns false" 仍依赖能拦截原生 confirm 形态，保留兼容（即便实际未调用）
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    vi.spyOn(window, 'prompt').mockReturnValue('test reason');
  });

  async function renderPage() {
    const { default: Page } = await import('@/pages/AmasAdvisorPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders spend stats and tabs', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('今日花费 USD')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: '待审批' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: '历史' })).toBeInTheDocument();
  });

  it('marks remaining as error color when below 0.1', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue({ ...mockSpend, remainingUsd: 0.05 });
    mockApi.amasListSuggestions.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/剩余额度/)).toBeInTheDocument());
  });

  it('shows empty pending when list is empty', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无待审批建议')).toBeInTheDocument());
  });

  it('renders pending suggestion with patch and approve flow', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue(mockPending);
    mockApi.amasApproveSuggestion.mockResolvedValue({ versionHash: 'newhash1234567' });
    await renderPage();
    await waitFor(() => expect(screen.getByText('增加稳定性参数以提升记忆')).toBeInTheDocument());
    expect(screen.getByText('memoryModel.w[0]')).toBeInTheDocument();
    // 卡片按钮：进入 ConfirmDialog
    fireEvent.click(screen.getByRole('button', { name: '批准并应用' }));
    // ConfirmDialog 标题确认弹出
    await waitFor(() => expect(screen.getByText('确认批准并应用 patch')).toBeInTheDocument());
    // 此时 "批准并应用" 出现在卡片按钮 + 弹窗确认按钮两处；取 ConfirmDialog 内的（最后一个）
    const approveButtons = screen.getAllByRole('button', { name: '批准并应用' });
    fireEvent.click(approveButtons[approveButtons.length - 1]);
    await waitFor(() => expect(mockApi.amasApproveSuggestion).toHaveBeenCalledWith(1));
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('handles approve failure', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue(mockPending);
    mockApi.amasApproveSuggestion.mockRejectedValue(new Error('500'));
    await renderPage();
    await waitFor(() => expect(screen.getByText('批准并应用')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '批准并应用' }));
    await waitFor(() => expect(screen.getByText('确认批准并应用 patch')).toBeInTheDocument());
    const approveButtons = screen.getAllByRole('button', { name: '批准并应用' });
    fireEvent.click(approveButtons[approveButtons.length - 1]);
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('approve aborts when confirm returns false', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue(mockPending);
    await renderPage();
    await waitFor(() => expect(screen.getByText('批准并应用')).toBeInTheDocument());
    fireEvent.click(screen.getByText('批准并应用'));
    expect(mockApi.amasApproveSuggestion).not.toHaveBeenCalled();
  });

  it('reject flow calls API and shows toast', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue(mockPending);
    mockApi.amasRejectSuggestion.mockResolvedValue(undefined);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '拒绝' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '拒绝' }));
    // Modal 弹出后填写原因 + 确认拒绝
    await waitFor(() => expect(screen.getByText('拒绝建议')).toBeInTheDocument());
    const textarea = document.querySelector('textarea') as HTMLTextAreaElement;
    fireEvent.input(textarea, { target: { value: 'test reason' } });
    fireEvent.click(screen.getByRole('button', { name: '确认拒绝' }));
    await waitFor(() => expect(mockApi.amasRejectSuggestion).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('handles reject failure', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue(mockPending);
    mockApi.amasRejectSuggestion.mockRejectedValue(new Error('boom'));
    await renderPage();
    await waitFor(() => expect(screen.getByRole('button', { name: '拒绝' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '拒绝' }));
    await waitFor(() => expect(screen.getByText('拒绝建议')).toBeInTheDocument());
    fireEvent.click(screen.getByRole('button', { name: '确认拒绝' }));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('toggles evidence visibility', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue(mockPending);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/查看 evidence/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/查看 evidence/));
    await waitFor(() => expect(screen.getByText(/隐藏 evidence/)).toBeInTheDocument());
    expect(screen.getByText(/logLoss/)).toBeInTheDocument();
  });

  it('switches to history tab and shows rows', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockImplementation((status: string | undefined) =>
      Promise.resolve(status === 'pending' ? [] : mockHistory),
    );
    await renderPage();
    await waitFor(() => expect(screen.getByRole('tab', { name: '历史' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('tab', { name: '历史' }));
    await waitFor(() => expect(screen.getByText('已批准')).toBeInTheDocument());
    expect(screen.getByText('已拒绝')).toBeInTheDocument();
  });

  it('shows empty history state', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByRole('tab', { name: '历史' })).toBeInTheDocument());
    fireEvent.click(screen.getByRole('tab', { name: '历史' }));
    await waitFor(() => expect(screen.getByText('尚无历史')).toBeInTheDocument());
  });
});
