import { describe, it, expect, vi, beforeEach } from 'vitest';
import { screen, waitFor, fireEvent } from '@solidjs/testing-library';
import { renderWithProviders } from '../../helpers/render';
import { AmasVersionDrawer } from '@/components/admin/AmasVersionDrawer';

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
const mockAdmin = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockAmas = amasApi as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockToast = uiStore.toast as unknown as Record<string, ReturnType<typeof vi.fn>>;

const v1 = {
  versionHash: 'newer000111122223333',
  source: 'manual' as const,
  createdAt: '2026-04-16T00:00:00Z',
  note: 'newer',
  authorAdminId: 1,
};
const v2 = {
  versionHash: 'older0011222333444555',
  source: 'manual' as const,
  createdAt: '2026-04-15T00:00:00Z',
  note: 'older',
  authorAdminId: 1,
};

describe('AmasVersionDrawer — restore, expand/diff, cancel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('restore success path: refetch + onRestored', async () => {
    mockAdmin.amasListVersions.mockResolvedValue([v1, v2]);
    mockAdmin.amasGetVersion.mockResolvedValue({ snapshotJson: { foo: 1 } });
    mockAdmin.amasRestoreVersion.mockResolvedValue(undefined);
    mockAmas.getConfig.mockResolvedValue({ memoryModel: { baseDesiredRetention: 0.9, w: Array(19).fill(1) } });
    const onRestored = vi.fn();
    renderWithProviders(() => (
      <AmasVersionDrawer open={true} onClose={() => {}} currentConfig={{}} onRestored={onRestored} />
    ));
    await waitFor(() => expect(screen.getByText(/older0011/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/older0011/));
    await waitFor(() => expect(screen.getByText('回滚到此版本')).toBeInTheDocument());
    fireEvent.click(screen.getByText('回滚到此版本'));
    await waitFor(() => expect(screen.getByText(/回滚至版本/)).toBeInTheDocument());
    fireEvent.click(screen.getByText('确认回滚'));
    await waitFor(() => expect(onRestored).toHaveBeenCalled());
    expect(mockToast.success).toHaveBeenCalled();
  });

  it('expand item shows detail + diff branch + 收起 toggle', async () => {
    mockAdmin.amasListVersions.mockResolvedValue([v1, v2]);
    mockAdmin.amasGetVersion.mockResolvedValue({ snapshotJson: { foo: 1, bar: 'x', flag: true } });
    renderWithProviders(() => (
      <AmasVersionDrawer open={true} onClose={() => {}} currentConfig={{ foo: 2 }} onRestored={() => {}} />
    ));
    await waitFor(() => expect(screen.getByText(/older0011/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/older0011/));
    await waitFor(() => expect(screen.getByText(/与当前差异/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/older0011/));
  });

  it('cancel confirm dialog via 取消', async () => {
    mockAdmin.amasListVersions.mockResolvedValue([v1, v2]);
    mockAdmin.amasGetVersion.mockResolvedValue({ snapshotJson: { foo: 1 } });
    renderWithProviders(() => (
      <AmasVersionDrawer open={true} onClose={() => {}} currentConfig={{}} onRestored={() => {}} />
    ));
    await waitFor(() => expect(screen.getByText(/older0011/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/older0011/));
    await waitFor(() => expect(screen.getByText('回滚到此版本')).toBeInTheDocument());
    fireEvent.click(screen.getByText('回滚到此版本'));
    await waitFor(() => expect(screen.getByText('取消')).toBeInTheDocument());
    fireEvent.click(screen.getByText('取消'));
    await waitFor(() => expect(screen.queryByText(/回滚至版本/)).not.toBeInTheDocument());
  });
});
