import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

vi.mock('@/api/admin', () => ({
  adminApi: { amasListWhitelist: vi.fn(), amasAddWhitelist: vi.fn(), amasDeleteWhitelist: vi.fn() },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { WhitelistPanel } from '@/pages/amas-advisor/WhitelistPanel';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const rows = [
  { path: 'memoryModel.baseDesiredRetention', minSafe: 0.8, maxSafe: 0.95 },
  { path: 'memoryModel.w0', minSafe: 0.1, maxSafe: 2 },
];

describe('WhitelistPanel', () => {
  beforeEach(() => vi.clearAllMocks());

  it('渲染白名单条目', async () => {
    mockApi.amasListWhitelist.mockResolvedValue(rows);
    render(() => <WhitelistPanel />);
    await waitFor(() => expect(screen.getByText('memoryModel.baseDesiredRetention')).toBeInTheDocument());
    expect(screen.getByText('memoryModel.w0')).toBeInTheDocument();
  });

  it('新增条目调 amasAddWhitelist', async () => {
    mockApi.amasListWhitelist.mockResolvedValue(rows);
    mockApi.amasAddWhitelist.mockResolvedValue({ path: 'memoryModel.x', minSafe: 0, maxSafe: 1 });
    render(() => <WhitelistPanel />);
    await waitFor(() => expect(screen.getByText('memoryModel.w0')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('memoryModel.xxx'), { target: { value: 'memoryModel.x' } });
    fireEvent.input(screen.getByLabelText('min'), { target: { value: '0' } });
    fireEvent.input(screen.getByLabelText('max'), { target: { value: '1' } });
    fireEvent.click(screen.getByText('添加'));
    await waitFor(() => expect(mockApi.amasAddWhitelist).toHaveBeenCalledWith(
      { path: 'memoryModel.x', minSafe: 0, maxSafe: 1 },
    ));
  });

  it('删除走 ConfirmDialog 确认后调 amasDeleteWhitelist', async () => {
    mockApi.amasListWhitelist.mockResolvedValue(rows);
    mockApi.amasDeleteWhitelist.mockResolvedValue({ deleted: true });
    render(() => <WhitelistPanel />);
    await waitFor(() => expect(screen.getByText('memoryModel.w0')).toBeInTheDocument());
    fireEvent.click(screen.getAllByRole('button', { name: /删除/ })[0]);
    fireEvent.click(screen.getByText('确认删除'));
    await waitFor(() => expect(mockApi.amasDeleteWhitelist).toHaveBeenCalledWith('memoryModel.baseDesiredRetention'));
  });
});
