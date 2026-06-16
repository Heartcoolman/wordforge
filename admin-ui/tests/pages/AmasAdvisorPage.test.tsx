import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

// 重设计后的 AmasAdvisorPage 直接调用以下 adminApi 方法；全部 mock 并给安全默认值
vi.mock('@/api/admin', () => ({
  adminApi: {
    amasAdvisorCost: vi.fn(),
    amasAdvisorCostDaily: vi.fn(),
    amasAdvisorConfig: vi.fn(),
    amasListSuggestions: vi.fn(),
    amasListCanaries: vi.fn(),
    amasListWhitelist: vi.fn(),
    amasAdvisorRun: vi.fn(),
    amasApproveAllSuggestions: vi.fn(),
    amasApproveSuggestion: vi.fn(),
    amasRejectSuggestion: vi.fn(),
    amasRollbackSuggestion: vi.fn(),
    amasScaleCanary: vi.fn(),
    amasPromoteCanary: vi.fn(),
    amasRollbackCanary: vi.fn(),
    amasAddWhitelist: vi.fn(),
    amasDeleteWhitelist: vi.fn(),
    amasUpdateAdvisorConfig: vi.fn(),
    amasExplainParam: vi.fn(),
    amasSandboxSuggestion: vi.fn(),
    amasExportSuggestionsCsv: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import AmasAdvisorPage from '@/pages/AmasAdvisorPage';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

// shape 对齐 AdvisorCostStats
const cost = {
  monthYuan: 4.21, monthCapYuan: 10, quotaPct: 0.421, forecastYuan: 6.84,
  avg7dCostYuan: 0.14, monthCalls: 31, acceptedCount: 47, rejectedCount: 6,
  acceptanceRate: 0.887, usdToCny: 7.2,
};
// shape 对齐 AdvisorConfig
const cfg = {
  model: 'deepseek-v2', pollCron: '0 */20 * * * *', apiKeyTail: 'f3a8', monthCapYuan: 500,
  autoApplyEnabled: false, autoApplyMaxPerDay: 1, autoApplyMinConfidence: 0.8,
  grayscaleSteps: [20, 60, 100] as [number, number, number], advisorEnabled: true,
};

describe('AmasAdvisorPage（重设计）', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApi.amasAdvisorCost.mockResolvedValue(cost);
    mockApi.amasAdvisorCostDaily.mockResolvedValue([]);
    mockApi.amasAdvisorConfig.mockResolvedValue(cfg);
    mockApi.amasListSuggestions.mockResolvedValue([]);
    mockApi.amasListCanaries.mockResolvedValue([]);
    mockApi.amasListWhitelist.mockResolvedValue([]);
  });

  it('渲染页头 + 成本面板 + 四态 tab', async () => {
    renderWithProviders(() => <AmasAdvisorPage />);
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    // 本月成本：monthYuan 4.21 经 toFixed(1) 渲染为 ¥4.2
    await waitFor(() => expect(screen.getByText('¥4.2')).toBeInTheDocument());
    expect(screen.getByText('本月成本')).toBeInTheDocument();
    expect(screen.getByText('每日成本趋势')).toBeInTheDocument();
    // Tabs 渲染为 <button class="tab">，accessible role 为 button
    expect(screen.getByRole('button', { name: /待审建议/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /灰度中/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /历史/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /白名单/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /顾问配置/ })).toBeInTheDocument();
  });

  it('成本接口失败时成本面板降级而不整页崩', async () => {
    mockApi.amasAdvisorCost.mockRejectedValue(new Error('boom'));
    renderWithProviders(() => <AmasAdvisorPage />);
    expect(await screen.findByText('LLM 调参顾问')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('成本信息加载失败')).toBeInTheDocument());
    // 页头与 tab 仍在，整页未崩
    expect(screen.getByRole('button', { name: /待审建议/ })).toBeInTheDocument();
  });
});
