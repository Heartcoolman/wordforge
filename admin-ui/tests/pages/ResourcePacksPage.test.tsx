import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@solidjs/testing-library';
import { renderWithProviders } from '../helpers/render';

vi.mock('@/api/packs', () => ({
  packsApi: {
    list: vi.fn(),
    uploadVersion: vi.fn(),
    setActive: vi.fn(),
    deactivateVersion: vi.fn(),
    stats: vi.fn(),
  },
}));

vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { packsApi } from '@/api/packs';
const mockApi = packsApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

describe('ResourcePacksPage', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderPage() {
    const { default: Page } = await import('@/pages/ResourcePacksPage');
    return renderWithProviders(() => <Page />);
  }

  it('renders hero title "资源包"', async () => {
    mockApi.list.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('资源包')).toBeInTheDocument());
  });

  it('shows Empty state when API returns no packs', async () => {
    mockApi.list.mockResolvedValue([]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('暂无资源包')).toBeInTheDocument());
  });

  it('renders pack selector when API returns ≥1 pack', async () => {
    mockApi.list.mockResolvedValue([
      {
        packId: 'visual-fatigue',
        description: null,
        createdAt: '2026-01-01T00:00:00Z',
        updatedAt: '2026-01-01T00:00:00Z',
        versions: [],
      },
    ]);
    await renderPage();
    await waitFor(() => expect(screen.getByText('visual-fatigue')).toBeInTheDocument());
  });

  it('shows stable/beta channel tabs', async () => {
    mockApi.list.mockResolvedValue([
      {
        packId: 'p1',
        description: null,
        createdAt: '2026-01-01',
        updatedAt: '2026-01-01',
        versions: [],
      },
    ]);
    await renderPage();
    await waitFor(() => {
      expect(screen.getByRole('tab', { name: 'Stable' })).toBeInTheDocument();
      expect(screen.getByRole('tab', { name: 'Beta' })).toBeInTheDocument();
    });
  });
});
