import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

vi.mock('@/api/admin', () => ({
  adminApi: { amasAdvisorConfig: vi.fn(), amasUpdateAdvisorConfig: vi.fn(), getSettings: vi.fn(), updateSettings: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { AdvisorConfigPanel } from '@/pages/amas-advisor/AdvisorConfigPanel';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const cfg = {
  model: 'deepseek-chat', pollCron: '0 */20 * * * *', apiKeyTail: 'a1b2',
  monthCapYuan: 10, autoApplyEnabled: false, autoApplyMaxPerDay: 2,
  autoApplyMinConfidence: 0.85, grayscaleSteps: [20, 60, 100], advisorEnabled: true,
};

describe('AdvisorConfigPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // E3:AdvisorConfigPanel 现额外从 SystemSettings 读/写 canary 两阈值。
    mockApi.getSettings.mockResolvedValue({ canaryRewardDropThreshold: 0.05, canaryAnomalyRiseThreshold: 0.05 });
    mockApi.updateSettings.mockResolvedValue({});
  });

  it('渲染只读 model / 脱敏 API Key 尾号', async () => {
    mockApi.amasAdvisorConfig.mockResolvedValue(cfg);
    render(() => <AdvisorConfigPanel />);
    await waitFor(() => expect(screen.getByText('deepseek-chat')).toBeInTheDocument());
    expect(screen.getByText(/••••a1b2/)).toBeInTheDocument();
  });

  it('保存调 amasUpdateAdvisorConfig 带改动后的 monthCapYuan', async () => {
    mockApi.amasAdvisorConfig.mockResolvedValue(cfg);
    mockApi.amasUpdateAdvisorConfig.mockResolvedValue({ ...cfg, monthCapYuan: 20 });
    render(() => <AdvisorConfigPanel />);
    await waitFor(() => expect(screen.getByText('deepseek-chat')).toBeInTheDocument());
    const cap = screen.getByLabelText('月成本上限（¥）') as HTMLInputElement;
    fireEvent.input(cap, { target: { value: '20' } });
    fireEvent.click(screen.getByText('保存配置'));
    await waitFor(() => expect(mockApi.amasUpdateAdvisorConfig).toHaveBeenCalledWith(
      expect.objectContaining({ monthCapYuan: 20 }),
    ));
  });
});
