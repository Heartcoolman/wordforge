import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

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
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockSpend = {
  todayCostUsd: 0.123, dailyCapUsd: 5, remainingUsd: 4.877,
  todayTokensInput: 1000, todayTokensOutput: 800,
};

const pendingSuggestion = {
  id: 1, status: 'pending' as const,
  basedOnVersionHash: 'abc1234567def890',
  rationale: 'r', evidenceJson: { foo: 'bar' },
  patchJson: { 'memoryModel.someField': 'string-value' },
  costUsd: 0.01,
  decidedBy: null,
  createdAt: '2026-04-15T10:00:00Z',
};

const historyWithNullDecidedBy = {
  id: 3, status: 'rejected' as const,
  basedOnVersionHash: 'def4567890abcdef12',
  rationale: 'rejected one',
  evidenceJson: {},
  patchJson: { x: 1 },
  costUsd: null,
  decidedBy: null,
  createdAt: '2026-04-15T11:00:00Z',
};

async function renderPage() {
  const { default: Page } = await import('@/pages/admin/AmasAdvisorPage');
  return renderWithProviders(() => <Page />);
}

describe('AmasAdvisorPage — confirm cancel, history view, evidence toggle', () => {
  beforeEach(() => vi.clearAllMocks());

  it('approve flow window.confirm cancel: skips API call', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue([pendingSuggestion]);
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    await renderPage();
    await waitFor(() => expect(screen.getByText('批准并应用')).toBeInTheDocument());
    fireEvent.click(screen.getByText('批准并应用'));
    expect(mockApi.amasApproveSuggestion).not.toHaveBeenCalled();
  });

  it('switches to history tab; renders null decidedBy & null costUsd as "—"', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockImplementation((status) =>
      Promise.resolve(status === 'pending' ? [] : [historyWithNullDecidedBy]),
    );
    await renderPage();
    await waitFor(() => expect(screen.getByText(/历史/)).toBeInTheDocument());
    fireEvent.click(screen.getByRole('tab', { name: /历史/ }));
    await waitFor(() => expect(screen.getByText('rejected one')).toBeInTheDocument());
    expect(screen.getAllByText('—').length).toBeGreaterThanOrEqual(2);
  });

  it('toggles evidence visibility', async () => {
    mockApi.amasSuggestionSpend.mockResolvedValue(mockSpend);
    mockApi.amasListSuggestions.mockResolvedValue([pendingSuggestion]);
    await renderPage();
    await waitFor(() => expect(screen.getByText(/查看 evidence/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/查看 evidence/));
    await waitFor(() => expect(screen.getByText(/隐藏 evidence/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/隐藏 evidence/));
    await waitFor(() => expect(screen.getByText(/查看 evidence/)).toBeInTheDocument());
  });
});
