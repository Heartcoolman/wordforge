import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/amas', () => ({
  amasApi: {
    getConfig: vi.fn(),
    updateConfig: vi.fn(),
    getMetrics: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { amasApi } from '@/api/amas';

const mockAmasApi = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const mockConfig = { algorithm: 'sm2', interval: 1.5, easeFactor: 2.5 };
const mockMetrics = { avgRetention: 0.85, totalReviews: 10000, avgInterval: 5.2 };

describe('AmasConfigPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function renderPage() {
    const { default: AmasConfigPage } = await import('@/pages/admin/AmasConfigPage');
    return renderWithProviders(() => <AmasConfigPage />);
  }

  it('shows "AMAS 配置" heading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    expect(screen.getByText('AMAS 配置')).toBeInTheDocument();
  });

  it('shows loading spinner initially', async () => {
    mockAmasApi.getConfig.mockReturnValue(new Promise(() => {}));
    mockAmasApi.getMetrics.mockReturnValue(new Promise(() => {}));
    await renderPage();
    expect(screen.getByRole('status')).toBeInTheDocument();
  });

  it('shows "配置编辑器" heading after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('配置编辑器')).toBeInTheDocument();
    });
  });

  it('shows "保存配置" button after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: '保存配置' })).toBeInTheDocument();
    });
  });

  it('shows "算法指标" section after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByText('算法指标')).toBeInTheDocument();
    });
  });

  it('shows textarea with config JSON after loading', async () => {
    mockAmasApi.getConfig.mockResolvedValue(mockConfig);
    mockAmasApi.getMetrics.mockResolvedValue(mockMetrics);
    await renderPage();
    await waitFor(() => {
      const textarea = document.querySelector('textarea');
      expect(textarea).toBeInTheDocument();
      expect(textarea!.value).toContain('algorithm');
    });
  });
});
