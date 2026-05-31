import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@solidjs/testing-library';
import { createSignal, Show } from 'solid-js';

vi.mock('@/api/admin', () => ({
  adminApi: {
    amasCreateCanary: vi.fn(),
    amasScaleCanary: vi.fn(),
    amasRollbackCanary: vi.fn(),
  },
}));
vi.mock('@/stores/ui', () => ({
  uiStore: { toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() } },
}));

import { adminApi, type AmasSuggestion, type PatchCanaryWithMetrics, type WhitelistRow } from '@/api/admin';
import { SuggestionCard } from '@/pages/amas-advisor/SuggestionCard';
import { PatchCanaryCard } from '@/pages/amas-advisor/PatchCanaryCard';
const mockApi = adminApi as unknown as Record<string, ReturnType<typeof vi.fn>>;

const sug: AmasSuggestion = {
  id: 9, createdAt: '2026-05-29T10:00:00Z', basedOnVersionHash: 'abc1234567def890',
  patchJson: { 'memoryModel.baseDesiredRetention': 0.9 }, rationale: '关键路径建议',
  evidenceJson: {}, status: 'pending', decidedBy: null, decidedAt: null, decisionNote: null,
  costUsd: 0.01, tokensInput: 1, tokensOutput: 1, confidence: 0.9,
  baseValuesJson: { 'memoryModel.baseDesiredRetention': 0.88 },
};
const whitelist: WhitelistRow[] = [{ path: 'memoryModel.baseDesiredRetention', minSafe: 0.8, maxSafe: 0.95 }];

// 轻量装配 harness：进灰度 → 出现 canary 卡 → 扩量/回滚
function Harness() {
  // 真实页面 canary 卡数据来自 list 端点(含 metrics);本 harness 复用 create/scale 返回(基础
  // PatchCanary)做展示,运行期 mock 对象含 metrics 字段,故 cast 为 WithMetrics 供卡片渲染。
  const [canary, setCanary] = createSignal<PatchCanaryWithMetrics | null>(null);
  async function onCanary() {
    const c = await adminApi.amasCreateCanary({ suggestionId: sug.id, percent: 20 });
    setCanary(c as PatchCanaryWithMetrics);
  }
  async function onScale(percent: number) {
    const c = await adminApi.amasScaleCanary(canary()!.id, percent);
    setCanary(c as PatchCanaryWithMetrics);
  }
  async function onRollback() {
    await adminApi.amasRollbackCanary(canary()!.id);
    setCanary((c) => (c ? { ...c, status: 'rolled_back' } : c));
  }
  return (
    <div>
      <SuggestionCard s={sug} whitelist={whitelist} busy={false}
        onApprove={() => {}} onReject={() => {}} onCanary={onCanary} />
      <Show when={canary()}>
        {(c) => (
          <PatchCanaryCard c={c()} steps={[20, 60, 100]} busy={false}
            onScale={onScale} onRollback={onRollback} onPromote={() => {}} />
        )}
      </Show>
    </div>
  );
}

const baseCanary: PatchCanaryWithMetrics = {
  id: 11, suggestionId: 9, versionHash: 'deadbeef1234', percent: 20,
  cohortLo: 0, cohortHi: 20, status: 'active', baselineMetricsJson: '{}',
  startedAt: '2026-05-29T10:00:00Z', updatedAt: '2026-05-29T10:00:00Z',
  liveReward: 0.5, liveAnomalyRate: 0.01, baselineReward: 0.5,
};

describe('Advisor 关键路径：进灰度 → 扩量 → 回滚', () => {
  beforeEach(() => vi.clearAllMocks());

  it('完整串联', async () => {
    mockApi.amasCreateCanary.mockResolvedValue(baseCanary);
    mockApi.amasScaleCanary.mockResolvedValue({ ...baseCanary, percent: 60, cohortHi: 60 });
    mockApi.amasRollbackCanary.mockResolvedValue({ rolledBack: true });

    render(() => <Harness />);

    // 1. 进灰度
    fireEvent.click(screen.getByText(/进灰度/));
    await waitFor(() => expect(screen.getByText('灰度中')).toBeInTheDocument());
    expect(mockApi.amasCreateCanary).toHaveBeenCalledWith({ suggestionId: 9, percent: 20 });

    // 2. 扩量到 60%
    fireEvent.click(screen.getByText(/扩量到 60%/));
    await waitFor(() => expect(screen.getByText(/60%/)).toBeInTheDocument());
    expect(mockApi.amasScaleCanary).toHaveBeenCalledWith(11, 60);

    // 3. 回滚
    fireEvent.click(screen.getByText('回滚'));
    await waitFor(() => expect(mockApi.amasRollbackCanary).toHaveBeenCalledWith(11));
  });
});
