import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListVersions: vi.fn(),
    amasGetVersion: vi.fn(),
    amasRestoreVersion: vi.fn(),
  },
}));
vi.mock('@/api/amas', () => ({
  amasApi: { getConfig: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { amasApi } from '@/api/amas';
import { uiStore } from '@/stores/ui';
const mockAdminApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmasApi = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const versions = [
  { versionHash: 'aaa11122233344455', source: 'manual', createdAt: '2026-04-16T00:00:00Z', note: 'latest', authorAdminId: 1 },
  { versionHash: 'bbb11122233344455', source: 'llm_suggested', createdAt: '2026-04-15T00:00:00Z', note: null, authorAdminId: 2 },
  { versionHash: 'ccc11122233344455', source: 'llm_auto', createdAt: '2026-04-14T00:00:00Z', note: 'auto', authorAdminId: 3 },
];

describe('AmasVersionDrawer', () => {
  beforeEach(() => vi.clearAllMocks());

  async function renderDrawer(props: Partial<Record<string, unknown>> = {}) {
    const { AmasVersionDrawer } = await import('@/components/admin/AmasVersionDrawer');
    return renderWithProviders(() => (
      <AmasVersionDrawer
        open={props.open !== false}
        onClose={(props.onClose as () => void) ?? (() => {})}
        currentConfig={(props.currentConfig as Record<string, unknown>) ?? {}}
        onRestored={(props.onRestored as (c: Record<string, unknown>) => void) ?? (() => {})}
      />
    ));
  }

  it('not rendered when open=false', async () => {
    await renderDrawer({ open: false });
    expect(screen.queryByText('版本历史')).not.toBeInTheDocument();
  });

  it('shows empty state when no versions', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue([]);
    await renderDrawer();
    await waitFor(() => expect(screen.getByText('尚无版本')).toBeInTheDocument());
  });

  it('lists versions when open', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue(versions);
    await renderDrawer();
    await waitFor(() => expect(screen.getByText('aaa1112223')).toBeInTheDocument());
    expect(screen.getByText('LLM 建议')).toBeInTheDocument();
    expect(screen.getByText('LLM 自动')).toBeInTheDocument();
    expect(screen.getByText('当前')).toBeInTheDocument();
  });

  it('expands a version to show diff and restore button', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue(versions);
    mockAdminApi.amasGetVersion.mockResolvedValue({
      snapshotJson: { memoryModel: { baseDesiredRetention: 0.85 } },
    });
    await renderDrawer({ currentConfig: { memoryModel: { baseDesiredRetention: 0.92 } } });
    await waitFor(() => expect(screen.getByText('bbb1112223')).toBeInTheDocument());
    fireEvent.click(screen.getByText('bbb1112223').closest('button')!);
    await waitFor(() => expect(screen.getByText('回滚到此版本')).toBeInTheDocument());
  });

  it('restore confirms then succeeds', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue(versions);
    mockAdminApi.amasGetVersion.mockResolvedValue({ snapshotJson: { x: 1 } });
    mockAdminApi.amasRestoreVersion.mockResolvedValue(undefined);
    mockAmasApi.getConfig.mockResolvedValue({ x: 1 });
    const onRestored = vi.fn();
    await renderDrawer({ currentConfig: {}, onRestored });
    await waitFor(() => expect(screen.getByText('bbb1112223')).toBeInTheDocument());
    fireEvent.click(screen.getByText('bbb1112223').closest('button')!);
    await waitFor(() => expect(screen.getByText('回滚到此版本')).toBeInTheDocument());
    fireEvent.click(screen.getByText('回滚到此版本'));
    await waitFor(() => expect(screen.getByText('确认回滚')).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认回滚'));
    await waitFor(() => expect(mockToast.success).toHaveBeenCalled());
    expect(onRestored).toHaveBeenCalled();
  });

  it('handles restore failure', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue(versions);
    mockAdminApi.amasGetVersion.mockResolvedValue({ snapshotJson: {} });
    mockAdminApi.amasRestoreVersion.mockRejectedValue(new Error('boom'));
    await renderDrawer();
    await waitFor(() => expect(screen.getByText('bbb1112223')).toBeInTheDocument());
    fireEvent.click(screen.getByText('bbb1112223').closest('button')!);
    await waitFor(() => expect(screen.getByText('回滚到此版本')).toBeInTheDocument());
    fireEvent.click(screen.getByText('回滚到此版本'));
    fireEvent.click(screen.getByText('确认回滚'));
    await waitFor(() => expect(mockToast.error).toHaveBeenCalled());
  });

  it('cancels restore dialog', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue(versions);
    mockAdminApi.amasGetVersion.mockResolvedValue({ snapshotJson: {} });
    await renderDrawer();
    await waitFor(() => expect(screen.getByText('bbb1112223')).toBeInTheDocument());
    fireEvent.click(screen.getByText('bbb1112223').closest('button')!);
    await waitFor(() => expect(screen.getByText('回滚到此版本')).toBeInTheDocument());
    fireEvent.click(screen.getByText('回滚到此版本'));
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText('确认回滚')).not.toBeInTheDocument());
  });

  it('collapses version when clicked again', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue(versions);
    mockAdminApi.amasGetVersion.mockResolvedValue({ snapshotJson: {} });
    await renderDrawer();
    await waitFor(() => expect(screen.getByText('bbb1112223')).toBeInTheDocument());
    const btn = screen.getByText('bbb1112223').closest('button')!;
    fireEvent.click(btn);
    await waitFor(() => expect(screen.getByText('回滚到此版本')).toBeInTheDocument());
    fireEvent.click(btn);
    await waitFor(() => expect(screen.queryByText('回滚到此版本')).not.toBeInTheDocument());
  });

  it('clicking × close calls onClose', async () => {
    mockAdminApi.amasListVersions.mockResolvedValue(versions);
    const onClose = vi.fn();
    await renderDrawer({ onClose });
    await waitFor(() => expect(screen.getByText('版本历史')).toBeInTheDocument());
    const closeBtn = screen.getByText('×');
    fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalled();
  });
});
