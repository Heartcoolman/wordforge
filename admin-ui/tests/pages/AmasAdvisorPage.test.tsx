import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListSuggestions: vi.fn(),
    amasListCanaries: vi.fn(),
    amasAdvisorCost: vi.fn(),
    amasAdvisorCostDaily: vi.fn(),
    amasAdvisorConfig: vi.fn(),
    amasListWhitelist: vi.fn(),
    amasAdvisorRun: vi.fn(),
    amasApproveAllSuggestions: vi.fn(),
    amasUpdateAdvisorConfig: vi.fn(),
    amasApproveSuggestion: vi.fn(),
    amasRejectSuggestion: vi.fn(),
    amasCreateCanary: vi.fn(),
    amasScaleCanary: vi.fn(),
    amasRollbackCanary: vi.fn(),
    amasPromoteCanary: vi.fn(),
    amasRollbackSuggestion: vi.fn(),
    amasAddWhitelist: vi.fn(),
    amasDeleteWhitelist: vi.fn(),
    amasExportSuggestionsCsv: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import AmasAdvisorPage from '@/pages/AmasAdvisorPage';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const cost = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 42.1, forecastYuan: 6.84,
  avg7dCostYuan: 0.14, monthCalls: 31, acceptedCount: 47, rejectedCount: 6, acceptanceRate: 0.887,
};
const cfg = {
  model: 'deepseek-v2', pollCron: '0 */20 * * * *', apiKeyTail: 'f3a8', monthCapYuan: 10,
  autoApplyEnabled: false, autoApplyMaxPerDay: 1, autoApplyMinConfidence: 0.8,
  grayscaleSteps: [20, 60, 100], advisorEnabled: true,
};

describe('AmasAdvisorPage（重设计）', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.amasListSuggestions.mockResolvedValue([]);
    mockApi.amasListCanaries.mockResolvedValue([]);
    mockApi.amasAdvisorCost.mockResolvedValue(cost);
    mockApi.amasAdvisorCostDaily.mockResolvedValue([]);
    mockApi.amasAdvisorConfig.mockResolvedValue(cfg);
    mockApi.amasListWhitelist.mockResolvedValue([]);
  });

  it('渲染 hero + 成本行 + 四态 tab', async () => {
    renderWithProviders(() => <AmasAdvisorPage />);
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('¥4.21')).toBeInTheDocument());
    expect(screen.getByRole('tab', { name: /待审/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /灰度中/ })).toBeInTheDocument();
  });

  it('成本接口失败时成本行降级而不整页崩', async () => {
    mockApi.amasAdvisorCost.mockRejectedValue(new Error('boom'));
    renderWithProviders(() => <AmasAdvisorPage />);
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/成本信息加载失败/)).toBeInTheDocument());
  });
});
