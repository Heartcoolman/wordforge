import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasListSuggestions: vi.fn(),
    amasRollbackSuggestion: vi.fn(),
    amasExportSuggestionsCsv: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi } from '@/api/admin';
import { HistoryTable } from '@/pages/amas-advisor/HistoryTable';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const item = {
  id: 5, createdAt: '2026-05-29T10:00:00Z', basedOnVersionHash: 'abc1234567def890',
  patchJson: { 'memoryModel.w0': 0.5 }, rationale: '历史一条',
  evidenceJson: {}, status: 'approved' as const, decidedBy: 'admin@x.com',
  decidedAt: '2026-05-29T11:00:00Z', decisionNote: null,
  costUsd: 0.02, tokensInput: 100, tokensOutput: 80, confidence: 0.9, baseValuesJson: null,
};

describe('HistoryTable', () => {
  beforeEach(() => vi.clearAllMocks());

  it('渲染历史行 + 默认 offset=0 查询', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    expect(mockApi.amasListSuggestions).toHaveBeenCalledWith(undefined, 50, 0, undefined);
  });

  it('搜索框输入 q 后重新查询', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    fireEvent.input(screen.getByPlaceholderText('搜索参数 / rationale…'), { target: { value: 'w0' } });
    fireEvent.click(screen.getByText('搜索'));
    await waitFor(() => expect(mockApi.amasListSuggestions).toHaveBeenLastCalledWith(undefined, 50, 0, 'w0'));
  });

  it('行级回滚走 ConfirmDialog → amasRollbackSuggestion', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    mockApi.amasRollbackSuggestion.mockResolvedValue({ rolledBack: true, versionHash: 'newhash' });
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    fireEvent.click(screen.getByText('回滚'));
    fireEvent.click(screen.getByText('确认回滚'));
    await waitFor(() => expect(mockApi.amasRollbackSuggestion).toHaveBeenCalledWith(5));
  });

  it('导出 CSV 调 amasExportSuggestionsCsv', async () => {
    mockApi.amasListSuggestions.mockResolvedValue([item]);
    mockApi.amasExportSuggestionsCsv.mockResolvedValue('id,created_at\n5,2026-05-29');
    // jsdom 无 URL.createObjectURL，桩掉避免下载触发报错
    (globalThis.URL as unknown as Record<string, unknown>).createObjectURL = vi.fn(() => 'blob:x');
    (globalThis.URL as unknown as Record<string, unknown>).revokeObjectURL = vi.fn();
    render(() => <HistoryTable />);
    await waitFor(() => expect(screen.getByText('历史一条')).toBeInTheDocument());
    fireEvent.click(screen.getByText('导出 CSV'));
    await waitFor(() => expect(mockApi.amasExportSuggestionsCsv).toHaveBeenCalled());
  });
});
