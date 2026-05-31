import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: { amasAnomalyFeed: vi.fn() },
}));
vi.mock('@/components/ui/EChart', () => ({ EChart: () => <div data-testid="chart" /> }));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const emptyFeed = {
  summary: { error: 0, warn: 0, info: 0, invariantPass: 120, invariantTotal: 120, passRate: 1 },
  items: [],
};
const fullFeed = {
  summary: { error: 2, warn: 3, info: 4, invariantPass: 115, invariantTotal: 120, passRate: 0.9583 },
  items: [
    {
      id: 'abc12345-def6-7890-ghij-klmnopqrstuv',
      timestamp: '2026-05-28T10:00:00Z',
      severity: 'error' as const,
      code: 'mastery_bounds',
      title: '掌握度越界',
      userId: 'user-0001-aaaa-bbbb-cccc',
      value: 1.234,
      expectedRange: '[0, 1]',
      impactedUsers: 7,
      impactPct: 0.123,
    },
  ],
};

describe('AnomaliesPanel', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPanel() {
    const { AnomaliesPanel } = await import('@/pages/amas/AnomaliesPanel');
    return renderWithProviders(() => <AnomaliesPanel />);
  }

  it('renders heading and window picker', async () => {
    mockApi.amasAnomalyFeed.mockResolvedValue(emptyFeed);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('异常 / 不变量违反')).toBeInTheDocument());
    expect(screen.getByText('7 天')).toBeInTheDocument();
    expect(screen.getByText('14 天')).toBeInTheDocument();
    expect(screen.getByText('30 天')).toBeInTheDocument();
  });

  it('shows empty state when no items', async () => {
    mockApi.amasAnomalyFeed.mockResolvedValue(emptyFeed);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('窗口内无异常事件')).toBeInTheDocument());
  });

  it('shows severity summary and items when populated', async () => {
    mockApi.amasAnomalyFeed.mockResolvedValue(fullFeed);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('error 级')).toBeInTheDocument());
    expect(screen.getByText('warn 级')).toBeInTheDocument();
    expect(screen.getByText('info 级')).toBeInTheDocument();
    expect(screen.getByText('不变量通过')).toBeInTheDocument();
    // 逐条异常：code 渲染
    expect(screen.getByText('mastery_bounds')).toBeInTheDocument();
  });

  it('switches days window via picker', async () => {
    mockApi.amasAnomalyFeed.mockResolvedValue(emptyFeed);
    await renderPanel();
    await waitFor(() => expect(screen.getByText('14 天')).toBeInTheDocument());
    fireEvent.click(screen.getByText('14 天'));
    await waitFor(() => expect(mockApi.amasAnomalyFeed).toHaveBeenCalledWith(14, 50));
  });
});
